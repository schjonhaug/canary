use crate::email_service::BatchEmailRequest;
use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use std::time::Duration;
use tokio::sync::mpsc;

/// Maximum emails per batch (Resend API limit)
const MAX_BATCH_SIZE: usize = 100;

/// Minimum delay between batch sends (500ms = 2 requests/second)
const BATCH_SEND_INTERVAL: Duration = Duration::from_millis(500);

/// Maximum retry attempts for failed batches
const MAX_RETRIES: u32 = 3;

/// Global email queue sender - initialized once during startup
static EMAIL_QUEUE_SENDER: OnceCell<mpsc::UnboundedSender<QueuedEmail>> = OnceCell::new();

/// A queued email with retry metadata
#[derive(Clone)]
struct QueuedEmail {
    request: BatchEmailRequest,
    retry_count: u32,
}

/// Configuration for the email queue
#[derive(Clone)]
pub struct EmailQueueConfig {
    pub resend_api_key: String,
    pub resend_from_email: String,
    pub resend_from_name: String,
}

impl EmailQueueConfig {
    pub fn from_env() -> Result<Self> {
        let resend_api_key = std::env::var("RESEND_API_KEY")
            .map_err(|_| anyhow!("RESEND_API_KEY environment variable not set"))?;
        let resend_from_email = std::env::var("RESEND_FROM_EMAIL")
            .map_err(|_| anyhow!("RESEND_FROM_EMAIL environment variable not set"))?;
        let resend_from_name = std::env::var("RESEND_FROM_NAME")
            .unwrap_or_else(|_| "Canary Bitcoin Wallet".to_string());

        Ok(Self {
            resend_api_key,
            resend_from_email,
            resend_from_name,
        })
    }
}

/// Queue an email for background sending
pub fn queue_email(email: BatchEmailRequest) -> Result<()> {
    let queued = QueuedEmail {
        request: email,
        retry_count: 0,
    };

    let sender = EMAIL_QUEUE_SENDER
        .get()
        .ok_or_else(|| anyhow!("Email queue not initialized"))?;

    sender
        .send(queued)
        .map_err(|e| anyhow!("Failed to queue email: {}", e))
}

/// Queue multiple emails for background sending
pub fn queue_emails(emails: Vec<BatchEmailRequest>) -> Result<()> {
    for email in emails {
        queue_email(email)?;
    }
    Ok(())
}

/// Start the email queue worker
/// This should be called once during application initialization
pub async fn start_email_queue_worker(config: EmailQueueConfig) -> Result<()> {
    // Create the channel
    let (sender, mut receiver) = mpsc::unbounded_channel();

    // Store the sender in the global static
    EMAIL_QUEUE_SENDER
        .set(sender)
        .map_err(|_| anyhow!("Email queue worker already started"))?;

    tokio::spawn(async move {
        println!("📧 Email queue worker started (rate limit: 2 requests/second)");

        let mut pending_emails = Vec::new();
        let mut last_send_time = tokio::time::Instant::now();

        loop {
            // Collect emails from the queue with a timeout
            // This allows us to send batches even if we don't reach MAX_BATCH_SIZE
            let timeout = tokio::time::sleep(Duration::from_millis(100));
            tokio::pin!(timeout);

            tokio::select! {
                Some(queued_email) = receiver.recv() => {
                    pending_emails.push(queued_email);

                    // If we've reached the batch size, send immediately (after rate limit)
                    if pending_emails.len() >= MAX_BATCH_SIZE {
                        send_batch_with_rate_limit(
                            &config,
                            &mut pending_emails,
                            &mut last_send_time,
                        ).await;
                    }
                }
                _ = &mut timeout => {
                    // Timeout reached - send any pending emails
                    if !pending_emails.is_empty() {
                        send_batch_with_rate_limit(
                            &config,
                            &mut pending_emails,
                            &mut last_send_time,
                        ).await;
                    }
                }
            }
        }
    });

    Ok(())
}

/// Send a batch of emails with rate limiting
async fn send_batch_with_rate_limit(
    config: &EmailQueueConfig,
    pending_emails: &mut Vec<QueuedEmail>,
    last_send_time: &mut tokio::time::Instant,
) {
    if pending_emails.is_empty() {
        return;
    }

    // Enforce rate limit: ensure at least BATCH_SEND_INTERVAL has passed
    let elapsed = last_send_time.elapsed();
    if elapsed < BATCH_SEND_INTERVAL {
        let sleep_duration = BATCH_SEND_INTERVAL - elapsed;
        tokio::time::sleep(sleep_duration).await;
    }

    // Take up to MAX_BATCH_SIZE emails
    let batch_size = pending_emails.len().min(MAX_BATCH_SIZE);
    let batch: Vec<QueuedEmail> = pending_emails.drain(0..batch_size).collect();

    // Extract requests for sending
    let requests: Vec<BatchEmailRequest> = batch.iter().map(|q| q.request.clone()).collect();

    // Send the batch
    match send_batch_to_resend(config, &requests).await {
        Ok(results) => {
            let success_count = results.iter().filter(|r| r.is_ok()).count();
            let failure_count = results.len() - success_count;

            if failure_count > 0 {
                println!("📧 Sent batch: {} succeeded, {} failed", success_count, failure_count);
            }

            // Handle individual failures - retry if possible
            for (idx, result) in results.into_iter().enumerate() {
                if let Err(e) = result {
                    if let Some(queued) = batch.get(idx) {
                        if queued.retry_count < MAX_RETRIES {
                            // Re-queue with incremented retry count
                            let mut retry_email = queued.clone();
                            retry_email.retry_count += 1;

                            eprintln!(
                                "⚠️  Retrying failed email (attempt {}/{}): {}",
                                retry_email.retry_count, MAX_RETRIES, e
                            );

                            // Add exponential backoff delay before re-queuing
                            let delay = Duration::from_millis(100 * (1 << retry_email.retry_count));
                            let retry_email_clone = retry_email.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(delay).await;
                                if let Some(sender) = EMAIL_QUEUE_SENDER.get() {
                                    if let Err(e) = sender.send(retry_email_clone) {
                                        eprintln!("❌ Failed to re-queue email: {}", e);
                                    }
                                }
                            });
                        } else {
                            eprintln!("❌ Email failed after {} retries: {}", MAX_RETRIES, e);
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to send email batch: {}", e);

            // Re-queue all emails for retry if under retry limit
            for queued in batch {
                if queued.retry_count < MAX_RETRIES {
                    let mut retry_email = queued.clone();
                    retry_email.retry_count += 1;

                    // Add exponential backoff delay
                    let delay = Duration::from_millis(100 * (1 << retry_email.retry_count));
                    let retry_email_clone = retry_email.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        if let Some(sender) = EMAIL_QUEUE_SENDER.get() {
                            if let Err(e) = sender.send(retry_email_clone) {
                                eprintln!("❌ Failed to re-queue email: {}", e);
                            }
                        }
                    });
                }
            }
        }
    }

    *last_send_time = tokio::time::Instant::now();
}

/// Send a batch of emails to Resend API
async fn send_batch_to_resend(
    config: &EmailQueueConfig,
    emails: &[BatchEmailRequest],
) -> Result<Vec<Result<String>>> {
    if emails.is_empty() {
        return Ok(Vec::new());
    }

    let from = format!("{} <{}>", config.resend_from_name, config.resend_from_email);

    // Build batch payload as JSON array
    let batch_payload: Vec<serde_json::Value> = emails
        .iter()
        .map(|req| {
            serde_json::json!({
                "from": from,
                "to": [format!("{} <{}>", req.to_name, req.to_email)],
                "subject": req.subject,
                "html": req.html_body,
                "text": req.text_body
            })
        })
        .collect();

    // Use raw HTTP API for batch sending
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.resend.com/emails/batch")
        .header("Authorization", format!("Bearer {}", config.resend_api_key))
        .header("Content-Type", "application/json")
        .json(&batch_payload)
        .send()
        .await?;

    let status = response.status();

    if status.is_success() {
        match response.json::<BatchSendResponse>().await {
            Ok(batch_response) => {
                // Map response IDs back to original order
                Ok(emails
                    .iter()
                    .enumerate()
                    .map(|(idx, _)| {
                        batch_response
                            .data
                            .get(idx)
                            .map(|email_data| email_data.id.clone())
                            .ok_or_else(|| {
                                anyhow!("Missing email ID in batch response at index {}", idx)
                            })
                    })
                    .collect())
            }
            Err(e) => {
                // Failed to parse response - return error for all
                Err(anyhow!("Failed to parse batch response: {}", e))
            }
        }
    } else {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(anyhow!("Batch send failed with status {}: {}", status, error_text))
    }
}

/// Response from Resend batch send API
#[derive(Debug, serde::Deserialize)]
struct BatchSendResponse {
    data: Vec<BatchEmailData>,
}

#[derive(Debug, serde::Deserialize)]
struct BatchEmailData {
    id: String,
}
