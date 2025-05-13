use std::time::Duration;

use crate::cfg::Cfg;
use anyhow::Context;
use async_impl::drain_strategy::DrainRequest;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use mempool::Transaction;
use tokio::select;

#[derive(Clone)]
pub struct SubmittanceSource(tokio::sync::mpsc::Sender<Transaction>);

pub async fn start_server(
    cfg: Cfg,
    submittance_source: SubmittanceSource,
    drain_request_source: DrainRequestSource,
) -> anyhow::Result<()> {
    let listener =
        tokio::net::TcpListener::bind(format!("0.0.0.0:{}", cfg.http_port.unwrap_or(8080))).await?;
    println!("HTTP server listening on {}", listener.local_addr()?);

    let app = build_router(submittance_source, drain_request_source);
    axum::serve(listener, app.into_make_service())
        .await
        .context("server crashed")
}

/// Submit the transaction transmitted in the request body to the managed priority queue.
/// The submitter waits at maximum for `timeout_us` before cancelling the operation and returning
/// the HTTP code 503 "busy".
#[axum::debug_handler]
async fn submit_transaction(
    State(SubmittanceSource(submitter)): State<SubmittanceSource>,
    Path(timeout_us): Path<u64>,
    Json(transaction): Json<Transaction>,
) -> impl IntoResponse {
    if let Err(e) = submitter
        .send_timeout(transaction, Duration::from_micros(timeout_us))
        .await
    {
        eprintln!("Logging submittance error: {e}");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "queue is under heavy load, could not add transaction",
        )
            .into_response();
    }

    StatusCode::OK.into_response()
}

/// Return type of drain request.
#[derive(Clone)]
pub struct DrainRequestSource(tokio::sync::mpsc::Sender<DrainRequest>);

#[derive(Debug, serde::Serialize)]
pub struct Drainage(Vec<Transaction>);

/// Tries to drain `n` elements from the queue with an timeout of `timeout_us` microseconds.
/// Should the timeout be reached without there being `n` elements to drain, all remaining elements are drained and
/// returned.
async fn drain_transactions(
    State(DrainRequestSource(drainage_requester)): State<DrainRequestSource>,
    Path((n, timeout_us)): Path<(usize, u64)>,
) -> impl IntoResponse {
    let (req, rx) = DrainRequest::new_with_timeout(n, timeout_us);
    let timeout = Duration::from_micros(timeout_us);

    // use interval to keep track of overall request duration and cancel it when `timeout` is reached.
    let mut interval = tokio::time::interval(timeout);
    interval.tick().await; // resolves immediately

    if let Err(e) = drainage_requester.send_timeout(req, timeout).await {
        eprintln!("Logging drainage error: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not drain").into_response();
    };

    select! {
     res = rx => {
        match res {
            Ok(v) => Json(Drainage(v)).into_response(),
            Err(e) => {
                 eprintln!("Logging drainage error: {e}");
                 (StatusCode::INTERNAL_SERVER_ERROR, "could not drain").into_response()
        }
    }
     }
     _= interval.tick() => {
         StatusCode::REQUEST_TIMEOUT.into_response()
     }
    }
}

fn build_router(
    submittance_source: SubmittanceSource,
    drain_request_source: DrainRequestSource,
) -> axum::Router {
    axum::Router::new()
        .route("/submit/{timeout_us}", post(submit_transaction))
        .with_state(submittance_source)
        .route("/drain/{n}/{timeout_us}", get(drain_transactions))
        .with_state(drain_request_source)
}
