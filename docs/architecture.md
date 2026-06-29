# SorobanPulse Architecture

This document provides a comprehensive overview of the SorobanPulse system architecture, including component interactions, data flow, and integration patterns.

## Architecture Diagram

![SorobanPulse System Architecture](architecture.svg)

> The diagram above shows all major components, data flow paths, the multi-replica advisory lock mechanism, and the notification/webhook delivery flow. An SVG copy is available at [`docs/architecture.svg`](architecture.svg) for embedding in external tooling.

## System Architecture Overview

SorobanPulse is a high-performance event indexing and notification system for the Stellar blockchain. It monitors smart contract events through the Stellar RPC, indexes them into a PostgreSQL database, and delivers real-time notifications to subscribers via multiple channels.

```mermaid
graph TB
    subgraph Stellar["Stellar Network"]
        RPC["Stellar RPC<br/>(testnet/public)"]
        SC["Smart Contracts<br/>(Soroban)"]
    end

    subgraph Core["SorobanPulse Core"]
        IDX["Indexer Service<br/>(Event Processor)"]
        API["REST API<br/>(Axum)"]
        SSE["Server-Sent Events<br/>(Real-time Stream)"]
    end

    subgraph Data["Data Layer"]
        POSTGRES["PostgreSQL Database<br/>(Event Storage)"]
        CACHE["In-Memory Cache<br/>(Moka)"]
    end

    subgraph Notification["Notification System"]
        NOTIF["Notification Engine<br/>(Multi-channel)"]
        EMAIL["Email<br/>(Lettre)"]
        WEBHOOK["Webhooks<br/>(HTTP)"]
        SMS["SMS<br/>(Twilio)"]
        PAGERDUTY["PagerDuty<br/>(Alerts)"]
    end

    subgraph Stream["Streaming Services"]
        KINESIS["AWS Kinesis<br/>(Optional)"]
        PUBSUB["GCP Pub/Sub<br/>(Optional)"]
        KAFKA["Kafka<br/>(Optional)"]
    end

    subgraph SDK["Client SDKs"]
        RSSDK["Rust SDK"]
        JSSDK["JavaScript SDK"]
        PYSDK["Python SDK"]
    end

    RPC -->|Block Transactions| IDX
    SC -->|Smart Contract Events| RPC
    IDX -->|Parse & Index| POSTGRES
    IDX -->|Cache Contracts| CACHE
    POSTGRES -->|Query| API
    API -->|Subscribe| SSE
    POSTGRES -->|Events| NOTIF
    NOTIF -->|Send| EMAIL
    NOTIF -->|Send| WEBHOOK
    NOTIF -->|Send| SMS
    NOTIF -->|Alert| PAGERDUTY
    NOTIF -->|Stream| KINESIS
    NOTIF -->|Stream| PUBSUB
    NOTIF -->|Stream| KAFKA
    API -->|REST| RSSDK
    SSE -->|Stream| JSSDK
    WEBHOOK -->|Payload| SDK

    style Stellar fill:#1f71f0
    style Core fill:#06d6a0
    style Data fill:#f76707
    style Notification fill:#e63946
    style Stream fill:#9d4edd
    style SDK fill:#00b4d8
```

## Component Descriptions

### Stellar Integration Layer

**Stellar RPC**
- Connects to Stellar's RPC endpoints (testnet or public network)
- Fetches ledgers and transactions containing smart contract events
- Provides XDR-encoded contract invocation data

### Indexer Service

The Indexer is the heart of SorobanPulse, responsible for:

1. **Event Polling**: Continuously polls Stellar RPC for new blocks
2. **XDR Parsing**: Parses XDR-encoded contract invocation data
3. **Event Extraction**: Extracts contract events and their parameters
4. **Deduplication**: Uses Bloom filters to prevent processing duplicate events
5. **Content Filtering**: Applies user-defined filters to events
6. **Transformation**: Applies Lua transformations for custom processing
7. **Storage**: Persists events to PostgreSQL

### API Layer

The REST API provides:
- Event querying with pagination and filtering
- Subscription management
- Webhook configuration
- Real-time SSE streaming
- Administrative endpoints

**Key Technologies**:
- **Framework**: Axum web framework
- **Database**: SQLx for type-safe queries
- **Validation**: OpenAPI/Swagger documentation

### Data Layer

**PostgreSQL Database**
- Stores indexed events
- Maintains subscription and webhook metadata
- Stores notification delivery logs
- Supports full-text search and complex queries

**In-Memory Cache (Moka)**
- Caches smart contract metadata
- Reduces database load for frequently accessed contracts
- Configurable TTL for cache invalidation

### Notification System

Multi-channel notification delivery with:

1. **Email Notifications** (Lettre)
   - SMTP integration with DKIM signing
   - SPF/DKIM/DMARC validation
   - HTML and plain-text templates
   - Multi-language support (Handlebars)

2. **Webhooks**
   - HTTP POST delivery with retry logic
   - HMAC signature verification
   - Custom headers and payload transformation
   - Rate limiting per webhook

3. **SMS Notifications** (Twilio)
   - Short-form messages for critical alerts
   - Character limit optimization

4. **PagerDuty Integration**
   - Incident creation for critical events
   - On-call escalation support

5. **Streaming Services**
   - AWS Kinesis for high-throughput streaming
   - GCP Pub/Sub for multi-region delivery
   - Kafka for self-hosted deployments

## Event Flow

```mermaid
sequenceDiagram
    participant RPC as Stellar RPC
    participant IDX as Indexer Service
    participant DB as PostgreSQL
    participant API as REST API
    participant NOTIF as Notification Engine
    participant DEST as Notification Destination

    RPC->>IDX: New Block with Contract Events
    IDX->>IDX: Parse XDR
    IDX->>IDX: Extract Contract Events
    IDX->>IDX: Apply Content Filters
    IDX->>IDX: Deduplicate (Bloom Filter)
    IDX->>DB: Store Events
    DB->>API: Event Available
    API->>API: Notify Subscribers (SSE)
    DB->>NOTIF: Event Matches Subscription
    NOTIF->>NOTIF: Format Message
    NOTIF->>NOTIF: Apply Rate Limiting
    NOTIF->>DEST: Send Notification
    DEST-->>NOTIF: Delivery Status
    NOTIF->>DB: Log Delivery
```

## Multi-Replica Advisory Lock Mechanism

For systems with multiple SorobanPulse instances, advisory locks prevent duplicate event processing:

```mermaid
graph LR
    IDX1["Indexer Instance 1"]
    IDX2["Indexer Instance 2"]
    IDX3["Indexer Instance 3"]
    DB["PostgreSQL<br/>(Advisory Lock)"]

    IDX1 -->|Acquire Lock| DB
    IDX2 -->|Request Lock| DB
    IDX3 -->|Request Lock| DB
    DB -->|Granted| IDX1
    DB -->|Waiting| IDX2
    DB -->|Waiting| IDX3
    IDX1 -->|Process Block 12345| DB
    IDX1 -->|Release Lock| DB
    DB -->|Granted| IDX2
    IDX2 -->|Process Block 12346| DB
    IDX2 -->|Release Lock| DB
    DB -->|Granted| IDX3
    IDX3 -->|Process Block 12347| DB

    style IDX1 fill:#06d6a0
    style IDX2 fill:#ffd60a
    style IDX3 fill:#fd7792
```

**How It Works**:
1. Each indexer attempts to acquire an exclusive lock on a block sequence number
2. Only one instance can hold the lock at a time
3. The lock holder processes that block and its events
4. After processing, the lock is released
5. The next waiting instance acquires the lock and processes the next block
6. This ensures exactly-once event processing across replicas

## Subscription and Webhook Delivery

```mermaid
graph TB
    USER["User"]
    API["REST API"]
    DB["PostgreSQL"]
    WH["Webhook Manager"]
    RETRY["Retry Queue"]
    DEST["Webhook Endpoint"]

    USER -->|Create Subscription| API
    USER -->|Add Webhook| API
    API -->|Store Config| DB
    DB -->|Event Matches Filter| WH
    WH -->|Create Delivery Task| RETRY
    RETRY -->|Attempt 1| DEST
    DEST -->|Timeout/5xx| RETRY
    RETRY -->|Wait + Backoff| RETRY
    RETRY -->|Attempt 2| DEST
    DEST -->|Success| DB
    DB -->|Log Delivery| DB

    style USER fill:#e0aaff
    style API fill:#06d6a0
    style WH fill:#e63946
    style DEST fill:#00b4d8
```

**Delivery Guarantees**:
- **At-least-once delivery**: Retries with exponential backoff
- **Idempotency**: Webhook payloads include unique event IDs
- **Ordering**: Events are delivered in ledger sequence order per subscription
- **Rate limiting**: Per-webhook throughput limits prevent overwhelming endpoints

## Deployment Architecture

```mermaid
graph TB
    subgraph K8s["Kubernetes Cluster"]
        INGRESS["Ingress Controller<br/>(TLS/HTTP)"]
        POD1["SorobanPulse Pod 1"]
        POD2["SorobanPulse Pod 2"]
        POD3["SorobanPulse Pod 3"]
    end

    subgraph External["External Services"]
        PGDB["PostgreSQL<br/>(CloudSQL/RDS)"]
        CACHE["Redis<br/>(Optional)"]
    end

    subgraph RPC["Stellar RPC"]
        TESTNET["Testnet RPC"]
        PUBLIC["Public Network RPC"]
    end

    INGRESS -->|/api/*| POD1
    INGRESS -->|/api/*| POD2
    INGRESS -->|/api/*| POD3
    POD1 -->|Query| PGDB
    POD2 -->|Query| PGDB
    POD3 -->|Query| PGDB
    POD1 -->|Cache| CACHE
    POD2 -->|Cache| CACHE
    POD3 -->|Cache| CACHE
    POD1 -->|Poll| TESTNET
    POD2 -->|Poll| PUBLIC
    POD3 -->|Poll| PUBLIC

    style K8s fill:#e8f4f8
    style External fill:#fff3e0
    style RPC fill:#1f71f0
```

## Technology Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Language | Rust | Performance & type safety |
| Web Framework | Axum | HTTP server & routing |
| Database | PostgreSQL | Event storage & subscriptions |
| ORM | SQLx | Type-safe SQL queries |
| Cache | Moka | In-memory contract metadata cache |
| Logging | Tracing | Structured logging |
| Metrics | Prometheus | System observability |
| OpenTelemetry | OpenTelemetry | Distributed tracing |
| Email | Lettre | SMTP notifications |
| Streaming | Kinesis/Pub/Sub | High-throughput event streaming |
| Scripting | Lua/MLua | Event transformation |
| Bloom Filter | bloomfilter | Deduplication |
| Validation | jsonschema | Event schema validation |

## Scaling Considerations

### Horizontal Scaling

1. **Stateless API Pods**: Deploy multiple API instances behind a load balancer
2. **Distributed Indexing**: Use advisory locks to safely scale indexer instances
3. **Database Pooling**: Connection pooling with PgBouncer for high concurrency

### Performance Optimization

1. **Event Batching**: Process events in configurable batch sizes
2. **Index Optimization**: Database indexes on frequently filtered columns
3. **Cache Strategy**: Smart caching of contract metadata with TTL
4. **Compression**: GZIP compression for large response payloads

### Resilience

1. **Retry Logic**: Exponential backoff for transient failures
2. **Circuit Breakers**: Fail-fast on persistent external service failures
3. **Health Checks**: Liveness and readiness probes for orchestration
4. **Graceful Shutdown**: Complete in-flight requests before terminating

## Security Architecture

```mermaid
graph LR
    CLIENT["Client"]
    TLS["TLS/HTTPS"]
    API["API Server"]
    AUTH["Auth Layer"]
    DB["Database"]
    VAULT["Secrets Vault"]

    CLIENT -->|Encrypted| TLS
    TLS -->|Decrypt| API
    API -->|Verify Token| AUTH
    AUTH -->|Get Secret| VAULT
    VAULT -->|Return Secret| AUTH
    AUTH -->|Proceed| API
    API -->|Encrypted Connection| DB
```

**Security Features**:
- TLS 1.3 for all external connections
- JWT token validation for API endpoints
- HMAC signature verification for webhooks
- Secrets encryption at rest
- Rate limiting to prevent abuse
- Input validation for all user inputs

## Future Architecture Enhancements

1. **GraphQL API**: Alternative to REST for flexible querying
2. **Event Sourcing**: Event-driven architecture for audit trails
3. **Sharding**: Horizontal partition of events for extreme scale
4. **Machine Learning**: Anomaly detection for event patterns
5. **Multi-chain**: Support for additional blockchain networks

## References

- [Stellar Developer Documentation](https://developers.stellar.org/)
- [Soroban Smart Contracts](https://soroban.stellar.org/)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/configuration/overview/)
- [PostgreSQL Performance Tuning](https://www.postgresql.org/docs/current/performance-tips.html)
