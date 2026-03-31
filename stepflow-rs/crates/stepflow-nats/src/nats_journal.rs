// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

//! NATS JetStream implementation of the [`ExecutionJournal`] trait.
//!
//! All journal events are stored in a single shared JetStream stream, with
//! each root run using a distinct subject (`journal.<uuid_hex>`). This
//! avoids the "stream-per-root-run" pattern that creates a Raft group per
//! stream in clustered NATS, limiting scalability.
//!
//! JetStream's global sequence numbers serve as the journal's
//! [`SequenceNumber`]s — they are monotonically increasing per root run
//! because they are a subset of the global total order.
//!
//! ## Subject scheme
//!
//! ```text
//! journal.<tenant_id>.<root_run_id_hex>
//! ```
//!
//! The stream captures `journal.<tenant_id>.>` so all root runs for a
//! tenant share a single stream. Multi-tenancy uses separate streams
//! (`JOURNAL_<tenant_id>`) to avoid subject overlap between tenants.
//!
//! ## GC strategy
//!
//! Completed runs can be purged per-subject via
//! `stream.purge().filter("journal.<id>")`. A `MaxAge` on the stream
//! config acts as a safety net for orphaned runs.

use async_nats::jetstream::context::{GetStreamError, GetStreamErrorKind};
use async_nats::jetstream::stream::{DirectGetError, DirectGetErrorKind};
use async_nats::jetstream::ErrorCode;
use chrono::Utc;
use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_state::{
    ExecutionJournal, JournalEntry, JournalEvent, JournalEventStream, RootJournalInfo,
    SequenceNumber, StateError,
};
use uuid::Uuid;

/// Default stream name when none is configured.
const DEFAULT_STREAM_NAME: &str = "STEPFLOW_JOURNAL";

/// Default tenant ID used until multi-tenancy is implemented.
const DEFAULT_TENANT: &str = "default";

/// Subject root. Full subjects are `journal.<tenant>.<root_run_id_hex>`.
const SUBJECT_ROOT: &str = "journal";

/// NATS JetStream-backed execution journal.
///
/// All root runs share a single JetStream stream. Per-run isolation is
/// achieved via subject filtering on consumers. JetStream's global
/// sequence numbers serve as journal [`SequenceNumber`]s.
pub struct NatsJournal {
    jetstream: async_nats::jetstream::Context,
    stream_name: String,
    tenant_id: String,
}

impl NatsJournal {
    /// Connect to a NATS server and return a journal handle.
    ///
    /// Uses the default stream name and tenant ID.
    pub async fn connect(url: &str) -> error_stack::Result<Self, StateError> {
        Self::connect_with_config(
            url,
            DEFAULT_STREAM_NAME.to_string(),
            DEFAULT_TENANT.to_string(),
        )
        .await
    }

    /// Connect to a NATS server with a custom stream name.
    pub async fn connect_with_stream(
        url: &str,
        stream_name: String,
    ) -> error_stack::Result<Self, StateError> {
        Self::connect_with_config(url, stream_name, DEFAULT_TENANT.to_string()).await
    }

    /// Connect to a NATS server with custom stream name and tenant ID.
    pub async fn connect_with_config(
        url: &str,
        stream_name: String,
        tenant_id: String,
    ) -> error_stack::Result<Self, StateError> {
        let client = async_nats::connect(url)
            .await
            .change_context(StateError::Internal)
            .attach_printable_lazy(|| format!("Failed to connect to NATS at {url}"))?;

        let jetstream = async_nats::jetstream::new(client);
        log::info!("NatsJournal connected to {url} (stream: {stream_name}, tenant: {tenant_id})");

        Ok(Self {
            jetstream,
            stream_name,
            tenant_id,
        })
    }

    /// Build a journal from an existing JetStream context.
    pub fn from_jetstream(
        jetstream: async_nats::jetstream::Context,
        stream_name: Option<String>,
        tenant_id: Option<String>,
    ) -> Self {
        Self {
            jetstream,
            stream_name: stream_name.unwrap_or_else(|| DEFAULT_STREAM_NAME.to_string()),
            tenant_id: tenant_id.unwrap_or_else(|| DEFAULT_TENANT.to_string()),
        }
    }

    /// Subject for a given root run: `journal.<tenant>.<uuid_hex>`.
    fn subject(&self, root_run_id: Uuid) -> String {
        format!("{SUBJECT_ROOT}.{}.{}", self.tenant_id, root_run_id.simple())
    }

    /// Wildcard subject capturing all journal events for this tenant.
    fn wildcard_subject(&self) -> String {
        format!("{SUBJECT_ROOT}.{}.>", self.tenant_id)
    }

    /// Subject prefix for stripping in `list_active_roots`.
    fn subject_prefix(&self) -> String {
        format!("{SUBJECT_ROOT}.{}.", self.tenant_id)
    }

    /// Ensure the shared journal stream exists.
    async fn ensure_stream(
        &self,
    ) -> error_stack::Result<async_nats::jetstream::stream::Stream, StateError> {
        self.jetstream
            .get_or_create_stream(async_nats::jetstream::stream::Config {
                name: self.stream_name.clone(),
                subjects: vec![self.wildcard_subject()],
                retention: async_nats::jetstream::stream::RetentionPolicy::Limits,
                allow_direct: true,
                ..Default::default()
            })
            .await
            .change_context(StateError::Internal)
            .attach_printable_lazy(|| {
                format!(
                    "Failed to ensure NATS journal stream '{}'",
                    self.stream_name
                )
            })
    }
}

/// Wire format for journal messages stored in NATS.
///
/// The sequence number is provided by JetStream and is not part of
/// the payload.
#[derive(serde::Serialize, serde::Deserialize)]
struct JournalPayload {
    run_id: Uuid,
    root_run_id: Uuid,
    timestamp: chrono::DateTime<Utc>,
    event: JournalEvent,
}

/// Returns `true` if a `GetStreamError` indicates the stream does not exist.
fn is_stream_not_found(err: &GetStreamError) -> bool {
    matches!(
        err.kind(),
        GetStreamErrorKind::JetStream(ref js_err)
            if js_err.error_code() == ErrorCode::STREAM_NOT_FOUND
    )
}

/// Returns `true` if a `DirectGetError` indicates no message was found for
/// the subject (as opposed to a transient/connectivity failure).
fn is_subject_not_found(err: &DirectGetError) -> bool {
    matches!(err.kind(), DirectGetErrorKind::NotFound)
}

impl ExecutionJournal for NatsJournal {
    fn initialize_journal(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async move {
            self.ensure_stream().await?;
            Ok(())
        }
        .boxed()
    }

    fn write(
        &self,
        root_run_id: Uuid,
        event: JournalEvent,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>> {
        async move {
            let run_id = event_run_id(&event);
            let payload = JournalPayload {
                run_id,
                root_run_id,
                timestamp: Utc::now(),
                event,
            };
            let data = serde_json::to_vec(&payload)
                .change_context(StateError::Serialization)
                .attach_printable("Failed to serialize journal event")?;

            let ack = self
                .jetstream
                .publish(self.subject(root_run_id), data.into())
                .await
                .change_context(StateError::Internal)
                .attach_printable("Failed to publish journal event")?
                .await
                .change_context(StateError::Internal)
                .attach_printable("NATS JetStream ack failed for journal write")?;

            Ok(SequenceNumber::new(ack.sequence))
        }
        .boxed()
    }

    fn stream_from(
        &self,
        root_run_id: Uuid,
        from_sequence: SequenceNumber,
    ) -> JournalEventStream<'_> {
        Box::pin(async_stream::stream! {
            let stream = match self.jetstream.get_stream(&self.stream_name).await {
                Ok(s) => s,
                Err(e) if is_stream_not_found(&e) => return,
                Err(e) => {
                    yield Err(error_stack::report!(StateError::Internal)
                        .attach_printable(format!("Failed to get journal stream: {e}")));
                    return;
                }
            };

            let subject = self.subject(root_run_id);

            // Snapshot the stop point: the last message for this subject at
            // the time of first poll. If no messages exist, return empty.
            let last_subject_seq = match stream.direct_get_last_for_subject(&subject).await {
                Ok(msg) => msg.sequence,
                Err(e) if is_subject_not_found(&e) => return,
                Err(e) => {
                    yield Err(error_stack::report!(StateError::Internal)
                        .attach_printable(format!(
                            "Failed to get last message for subject: {e}"
                        )));
                    return;
                }
            };

            // If the caller asks to start beyond the last message, nothing to yield.
            if from_sequence.value() > last_subject_seq {
                return;
            }

            // Create a filtered consumer starting at the requested sequence.
            let deliver_policy = if from_sequence.value() <= 1 {
                async_nats::jetstream::consumer::DeliverPolicy::All
            } else {
                async_nats::jetstream::consumer::DeliverPolicy::ByStartSequence {
                    start_sequence: from_sequence.value(),
                }
            };

            let consumer = match stream
                .create_consumer(async_nats::jetstream::consumer::pull::Config {
                    deliver_policy,
                    filter_subject: subject,
                    ack_policy: async_nats::jetstream::consumer::AckPolicy::None,
                    ..Default::default()
                })
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    yield Err(error_stack::report!(StateError::Internal)
                        .attach_printable(format!("Failed to create journal consumer: {e}")));
                    return;
                }
            };

            use futures::StreamExt as _;
            let mut messages = match consumer.messages().await {
                Ok(m) => m,
                Err(e) => {
                    yield Err(error_stack::report!(StateError::Internal)
                        .attach_printable(format!("Failed to open message stream: {e}")));
                    return;
                }
            };

            while let Some(msg_result) = messages.next().await {
                let msg = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        yield Err(error_stack::report!(StateError::Internal)
                            .attach_printable(format!("Error reading journal message: {e}")));
                        return;
                    }
                };

                let seq = msg.info().map(|i| i.stream_sequence).unwrap_or(0);

                let payload: JournalPayload = match serde_json::from_slice(&msg.payload) {
                    Ok(p) => p,
                    Err(e) => {
                        yield Err(error_stack::report!(StateError::Serialization)
                            .attach_printable(format!(
                                "Failed to deserialize journal entry: {e}"
                            )));
                        return;
                    }
                };

                yield Ok(JournalEntry {
                    run_id: payload.run_id,
                    root_run_id: payload.root_run_id,
                    sequence: SequenceNumber::new(seq),
                    timestamp: payload.timestamp,
                    event: payload.event,
                });

                // Stop after the snapshotted last message for this subject.
                if seq >= last_subject_seq {
                    break;
                }
            }
        })
    }

    fn latest_sequence(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<SequenceNumber>, StateError>> {
        async move {
            let stream = match self.jetstream.get_stream(&self.stream_name).await {
                Ok(s) => s,
                Err(e) if is_stream_not_found(&e) => return Ok(None),
                Err(e) => {
                    return Err(e)
                        .change_context(StateError::Internal)
                        .attach_printable("Failed to get journal stream for latest_sequence")
                }
            };

            let subject = self.subject(root_run_id);
            match stream.direct_get_last_for_subject(&subject).await {
                Ok(msg) => Ok(Some(SequenceNumber::new(msg.sequence))),
                Err(e) if is_subject_not_found(&e) => Ok(None),
                Err(e) => Err(e)
                    .change_context(StateError::Internal)
                    .attach_printable("Failed to get last message for latest_sequence"),
            }
        }
        .boxed()
    }

    fn list_active_roots(
        &self,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RootJournalInfo>, StateError>> {
        async move {
            use futures::TryStreamExt as _;

            let stream = match self.jetstream.get_stream(&self.stream_name).await {
                Ok(s) => s,
                Err(e) if is_stream_not_found(&e) => return Ok(Vec::new()),
                Err(e) => {
                    return Err(e)
                        .change_context(StateError::Internal)
                        .attach_printable("Failed to get journal stream for list_active_roots")
                }
            };

            // Use the subjects API to enumerate all subjects and their counts.
            let subjects_stream = stream
                .info_with_subjects(self.wildcard_subject())
                .await
                .change_context(StateError::Internal)
                .attach_printable("Failed to enumerate journal subjects")?;

            let mut roots = Vec::new();
            let mut subjects_stream = std::pin::pin!(subjects_stream);
            while let Some((subject, count)) = subjects_stream
                .try_next()
                .await
                .change_context(StateError::Internal)
                .attach_printable("Failed to iterate journal subjects")?
            {
                let prefix = self.subject_prefix();
                let uuid_hex = match subject.strip_prefix(&prefix) {
                    Some(hex) => hex,
                    None => continue,
                };
                let root_run_id = match Uuid::parse_str(uuid_hex) {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                // Get the last message for this subject to find the latest sequence.
                let latest_sequence = match stream.direct_get_last_for_subject(&subject).await {
                    Ok(msg) => SequenceNumber::new(msg.sequence),
                    Err(e) if is_subject_not_found(&e) => continue,
                    Err(e) => {
                        log::warn!(
                            "Failed to get last message for subject {subject}: {e}; skipping"
                        );
                        continue;
                    }
                };

                roots.push(RootJournalInfo {
                    root_run_id,
                    latest_sequence,
                    entry_count: count as u64,
                });
            }

            Ok(roots)
        }
        .boxed()
    }

    fn follow(&self, root_run_id: Uuid, from_sequence: SequenceNumber) -> JournalEventStream<'_> {
        Box::pin(async_stream::stream! {
            // Ensure the stream exists so the consumer can attach.
            if let Err(e) = self.ensure_stream().await {
                yield Err(e);
                return;
            }

            let stream = match self.jetstream.get_stream(&self.stream_name).await {
                Ok(s) => s,
                Err(e) => {
                    yield Err(error_stack::report!(StateError::Internal)
                        .attach_printable(format!("Failed to get journal stream: {e}")));
                    return;
                }
            };

            let deliver_policy = if from_sequence.value() <= 1 {
                async_nats::jetstream::consumer::DeliverPolicy::All
            } else {
                async_nats::jetstream::consumer::DeliverPolicy::ByStartSequence {
                    start_sequence: from_sequence.value(),
                }
            };

            // Ordered pull consumer with subject filter. The Ordered stream
            // automatically handles reconnection and sequence tracking, and
            // never terminates — new messages are delivered as they arrive.
            let subject = self.subject(root_run_id);
            let consumer = match stream
                .create_consumer(async_nats::jetstream::consumer::pull::OrderedConfig {
                    deliver_policy,
                    filter_subject: subject,
                    ..Default::default()
                })
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    yield Err(error_stack::report!(StateError::Internal)
                        .attach_printable(format!("Failed to create ordered consumer: {e}")));
                    return;
                }
            };

            let ordered: async_nats::jetstream::consumer::pull::Ordered = match consumer
                .messages()
                .await
            {
                Ok(o) => o,
                Err(e) => {
                    yield Err(error_stack::report!(StateError::Internal)
                        .attach_printable(format!("Failed to open follow stream: {e}")));
                    return;
                }
            };

            use futures::StreamExt as _;
            let mut ordered = std::pin::pin!(ordered);
            while let Some(msg_result) = ordered.next().await {
                let msg: async_nats::jetstream::Message = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        yield Err(error_stack::report!(StateError::Internal)
                            .attach_printable(format!("Error in follow stream: {e}")));
                        return;
                    }
                };

                let seq = msg.info().map(|i| i.stream_sequence).unwrap_or(0);

                let payload: JournalPayload = match serde_json::from_slice(&msg.payload) {
                    Ok(p) => p,
                    Err(e) => {
                        yield Err(error_stack::report!(StateError::Serialization)
                            .attach_printable(format!(
                                "Failed to deserialize follow entry: {e}"
                            )));
                        return;
                    }
                };

                yield Ok(JournalEntry {
                    run_id: payload.run_id,
                    root_run_id: payload.root_run_id,
                    sequence: SequenceNumber::new(seq),
                    timestamp: payload.timestamp,
                    event: payload.event,
                });
            }
        })
    }
}

/// Extract the primary run_id from a journal event.
///
/// Most events carry their own `run_id`. For `TasksStarted`, we use the
/// first run's ID (the event spans multiple runs but needs a single
/// envelope run_id).
fn event_run_id(event: &JournalEvent) -> Uuid {
    match event {
        JournalEvent::RootRunCreated { run_id, .. }
        | JournalEvent::StepsNeeded { run_id, .. }
        | JournalEvent::RunCompleted { run_id, .. }
        | JournalEvent::TaskCompleted { run_id, .. }
        | JournalEvent::StepsUnblocked { run_id, .. }
        | JournalEvent::ItemCompleted { run_id, .. }
        | JournalEvent::SubRunCreated { run_id, .. } => *run_id,
        JournalEvent::TasksStarted { runs } => {
            runs.first().map(|r| r.run_id).unwrap_or(Uuid::nil())
        }
    }
}
