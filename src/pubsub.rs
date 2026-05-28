//! Issue #264: Google Cloud Pub/Sub publisher integration.
//!
//! When `PUBSUB_PROJECT_ID` and `PUBSUB_TOPIC_ID` are configured, each indexed
//! event is published to the Pub/Sub topic as a JSON message with event metadata
//! as message attributes. Authentication uses Application Default Credentials
//! (ADC) via the `GOOGLE_APPLICATION_CREDENTIALS` environment variable.
//!
//! This module is compiled only when the `pubsub` feature flag is enabled.

use crate::metrics;
use crate::models::SorobanEvent;
use async_trait::async_trait;
use tracing::{error, info};

/// Trait for publishing events to Pub/Sub, enabling mock testing.
#[async_trait]
pub trait PubSubPublisher: Send + Sync {
    async fn publish(&self, event: &SorobanEvent) -> Result<(), String>;
}

// ── Real GCP Pub/Sub implementation ──────────────────────────────────────────

#[cfg(feature = "pubsub")]
pub mod gcp {
    use super::*;
    use google_cloud_pubsub::client::Publisher;
    use google_cloud_pubsub::model::Message;

    pub struct GcpPubSubPublisher {
        publisher: Publisher,
        topic: String,
        enable_ordering: bool,
    }

    impl GcpPubSubPublisher {
        /// Build a publisher using Application Default Credentials.
        pub async fn from_env(project_id: String, topic_id: String, enable_ordering: bool) -> Result<Self, String> {
            let topic = format!("projects/{project_id}/topics/{topic_id}");
            let publisher = Publisher::builder(&topic)
                .build()
                .await
                .map_err(|e| format!("Failed to create Pub/Sub publisher: {e}"))?;
            info!(topic = %topic, enable_ordering = %enable_ordering, "Pub/Sub publisher initialised");
            Ok(Self { publisher, topic, enable_ordering })
        }
    }

    #[async_trait]
    impl PubSubPublisher for GcpPubSubPublisher {
        async fn publish(&self, event: &SorobanEvent) -> Result<(), String> {
            let payload =
                serde_json::to_vec(event).map_err(|e| format!("serialisation error: {e}"))?;

            let ledger_str = event.ledger.to_string();
            let mut msg = Message::new().set_data(payload).set_attributes([
                ("contract_id", event.contract_id.as_str()),
                ("event_type", event.event_type.as_str()),
                ("tx_hash", event.tx_hash.as_str()),
                ("ledger", ledger_str.as_str()),
            ]);

            if self.enable_ordering {
                msg = msg.set_ordering_key(&event.contract_id);
                metrics::record_pubsub_ordering_key_set();
            }

            self.publisher.publish(msg).await.map_err(|e| {
                let msg = e.to_string();
                error!(topic = %self.topic, error = %msg, "Pub/Sub publish failed");
                metrics::record_pubsub_publish_failure();
                msg
            })?;

            Ok(())
        }
    }
}

// ── Shared helper used by the indexer ────────────────────────────────────────

/// Publish an event, logging and metering failures without propagating them.
pub async fn publish_event(publisher: &dyn PubSubPublisher, event: &SorobanEvent) {
    if let Err(e) = publisher.publish(event).await {
        error!(error = %e, "Failed to publish event to Pub/Sub");
        metrics::record_pubsub_publish_failure();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct MockPubSubPublisher {
        published: Arc<Mutex<Vec<(String, String, bool)>>>,
        fail: bool,
    }

    #[async_trait]
    impl PubSubPublisher for MockPubSubPublisher {
        async fn publish(&self, event: &SorobanEvent) -> Result<(), String> {
            if self.fail {
                return Err("mock error".to_string());
            }
            self.published
                .lock()
                .unwrap()
                .push((event.contract_id.clone(), event.event_type.clone(), false));
            Ok(())
        }
    }

    fn make_event() -> SorobanEvent {
        SorobanEvent {
            contract_id: "CABC123".into(),
            event_type: "contract".into(),
            tx_hash: "deadbeef".into(),
            ledger: 100,
            ledger_closed_at: "2026-04-27T00:00:00Z".into(),
            value: Value::Null,
            topic: None,
        }
    }

    #[tokio::test]
    async fn mock_publisher_records_event() {
        let published = Arc::new(Mutex::new(vec![]));
        let mock = MockPubSubPublisher {
            published: published.clone(),
            fail: false,
        };
        mock.publish(&make_event()).await.unwrap();
        let records = published.lock().unwrap();
        assert_eq!(records[0].0, "CABC123");
        assert_eq!(records[0].1, "contract");
    }

    #[tokio::test]
    async fn mock_publisher_returns_error_on_failure() {
        let mock = MockPubSubPublisher {
            fail: true,
            ..Default::default()
        };
        assert!(mock.publish(&make_event()).await.is_err());
    }

    #[tokio::test]
    async fn publish_event_does_not_panic_on_failure() {
        let mock = MockPubSubPublisher {
            fail: true,
            ..Default::default()
        };
        publish_event(&mock, &make_event()).await;
    }

    #[tokio::test]
    async fn publish_event_records_metadata() {
        let published = Arc::new(Mutex::new(vec![]));
        let mock = MockPubSubPublisher {
            published: published.clone(),
            fail: false,
        };
        let mut event = make_event();
        event.event_type = "system".into();
        publish_event(&mock, &event).await;
        let records = published.lock().unwrap();
        assert_eq!(records[0].1, "system");
    }
}
