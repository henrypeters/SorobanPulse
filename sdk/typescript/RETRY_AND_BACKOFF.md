# Retry Policy and Exponential Backoff in Soroban Pulse TypeScript SDK

This document explains how to configure and use the retry and backoff functionality in the Soroban Pulse TypeScript SDK.

## Overview

The SDK implements automatic retry with exponential backoff for transient failures, including:
- Network timeouts and connection errors
- HTTP 429 (Rate Limit) responses
- HTTP 5xx server errors (500, 502, 503, 504)

Retries are automatically applied with exponential backoff: 1s, 2s, 4s, 8s, 16s (default).

## Configuration

### Basic Setup with Defaults

```typescript
import { DefaultApi, Configuration } from "./index";

const config = new Configuration({
  basePath: "http://localhost:3000",
  // Retry configuration with defaults:
  // maxRetries: 3
  // retryOnStatus: [429, 500, 502, 503, 504]
  // retryInitialDelayMs: 1000 (1 second)
  // retryMaxDelayMs: 32000 (32 seconds)
});

const api = new DefaultApi(config);
```

### Custom Retry Configuration

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 5,                    // Retry up to 5 times
  retryOnStatus: [429, 503, 504],  // Only retry on these status codes
  retryInitialDelayMs: 500,         // Start with 500ms delay
  retryMaxDelayMs: 60000,           // Cap at 60 seconds
  onRetry: (attempt, delayMs, reason) => {
    console.log(
      `Retry ${attempt} after ${delayMs}ms: ${reason}`
    );
  },
});

const api = new DefaultApi(config);
```

## Retry Policies

### 1. Default Retry Policy (Balanced)

Best for most use cases:

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 3,                    // 3 retries
  retryInitialDelayMs: 1000,        // 1s initial
  retryMaxDelayMs: 32000,           // 32s max
  retryOnStatus: [429, 500, 502, 503, 504],
});
```

**Backoff sequence:** 1s, 2s, 4s, 8s, 16s

### 2. Aggressive Retry Policy

For critical operations that must succeed:

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 5,                    // 5 retries
  retryInitialDelayMs: 500,         // 500ms initial
  retryMaxDelayMs: 60000,           // 60s max
  retryOnStatus: [429, 500, 502, 503, 504],
  onRetry: (attempt, delayMs, reason) => {
    console.warn(`Aggressive retry ${attempt}: ${reason}`);
  },
});
```

**Backoff sequence:** 500ms, 1s, 2s, 4s, 8s, 16s, 32s, 60s, 60s

### 3. Conservative Retry Policy

For operations that should fail fast:

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 1,                    // Only 1 retry
  retryInitialDelayMs: 2000,        // 2s initial
  retryMaxDelayMs: 5000,            // 5s max
  retryOnStatus: [503],             // Only retry service unavailable
});
```

**Backoff sequence:** 2s, 5s

## Monitoring Retries

### Retry Callback

Use the `onRetry` callback to monitor and log retry attempts:

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  onRetry: (attempt, delayMs, reason) => {
    console.log(
      `Attempt ${attempt} failed: ${reason}. ` +
      `Retrying after ${delayMs}ms...`
    );
  },
});
```

### Example with Request Tracking

```typescript
let requestMetrics = {
  total: 0,
  retried: 0,
  failed: 0,
};

const config = new Configuration({
  basePath: "http://localhost:3000",
  onRetry: (attempt, delayMs, reason) => {
    requestMetrics.retried++;
    console.log(
      `Request retry (total retries: ${requestMetrics.retried}): ${reason}`
    );
  },
});

const api = new DefaultApi(config);

try {
  requestMetrics.total++;
  const events = await api.getEvents({ page: 1, limit: 20 });
  console.log(
    `Success! Total requests: ${requestMetrics.total}, ` +
    `retried: ${requestMetrics.retried}`
  );
} catch (error) {
  requestMetrics.failed++;
  console.error(`Request failed after retries: ${error.message}`);
}
```

## Advanced Usage

### Custom Retry-After Header Support

The SDK automatically respects the `Retry-After` header from the server:

```typescript
// Server responds with:
// HTTP 429
// Retry-After: 60
//
// The SDK will wait 60 seconds before retrying (instead of using exponential backoff)
```

### Combine with Streaming

Retry logic also applies to SSE streaming connections:

```typescript
const stream = api.streamEventsSSE({
  apiKey: "your-api-key",
  onMessage: (event) => {
    console.log("Event:", JSON.parse(event.data));
  },
  autoReconnect: true,
  maxReconnectAttempts: 5,
  // The underlying HTTP connection to establish the stream will also use retry logic
});

stream.connect();
```

### Exponential Backoff Formula

The SDK uses this formula for delay calculation:

```
delay = min(
  2^attempt * initialDelayMs + random(0, initialDelayMs),
  maxDelayMs
)
```

This ensures:
1. **Exponential growth:** Each retry waits roughly 2x longer
2. **Jitter:** Adds randomness to prevent thundering herd
3. **Cap:** Never exceeds `maxDelayMs`

**Example with defaults:**

| Attempt | Formula | Result |
|---------|---------|--------|
| 1 | 2^0 × 1000 + jitter | ~1000ms |
| 2 | 2^1 × 1000 + jitter | ~2000ms |
| 3 | 2^2 × 1000 + jitter | ~4000ms |
| 4 | 2^3 × 1000 + jitter | ~8000ms |
| 5 | 2^4 × 1000 + jitter | ~16000ms |
| 6+ | capped at 32000 | 32000ms |

## Error Scenarios

### Rate Limiting (HTTP 429)

```typescript
// Server: HTTP 429 Too Many Requests
// SDK: Retries with exponential backoff
// If Retry-After header is present, respects that instead

const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 3,
  retryOnStatus: [429],  // Retry on rate limit
  onRetry: (attempt, delayMs, reason) => {
    console.warn(`Rate limited! Retrying after ${delayMs}ms`);
  },
});
```

### Server Errors (5xx)

```typescript
// Server: HTTP 500, 502, 503, or 504
// SDK: Retries automatically

const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 5,  // More retries for server errors
  retryOnStatus: [500, 502, 503, 504],
  onRetry: (attempt, delayMs, reason) => {
    console.warn(`Server error (${reason}). Retrying...`);
  },
});
```

### Network Timeouts

Network timeouts will throw an error that's not automatically retried by the SDK's HTTP layer.
To add retry logic for timeouts, use middleware:

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  middleware: [
    {
      async pre(context) {
        // Custom timeout handling could be added here
        return context;
      },
    },
  ],
});
```

## Best Practices

1. **Use appropriate retry counts:** More retries = longer waits. Balance for your use case.

2. **Monitor retry metrics:** Log retry attempts to understand server health and adjust configuration.

3. **Set reasonable max delays:** Very long delays may timeout at the application level.

4. **Respect Retry-After headers:** The SDK does this automatically; don't override.

5. **Implement circuit breaker:** For production, consider adding a circuit breaker pattern on top:

```typescript
let failureCount = 0;
const FAILURE_THRESHOLD = 5;
let circuitOpen = false;

const config = new Configuration({
  basePath: "http://localhost:3000",
  onRetry: (attempt, delayMs, reason) => {
    failureCount++;
    if (failureCount >= FAILURE_THRESHOLD) {
      circuitOpen = true;
      console.error("Circuit breaker opened!");
    }
  },
});

// Before making requests
if (circuitOpen) {
  throw new Error("Circuit breaker is open");
}
```

## Examples

### Example 1: Fetching Events with Retry

```typescript
import { EventsApi, Configuration } from "@soroban/pulse-client";

const config = new Configuration({
  basePath: "https://api.sorobanpulse.com",
  maxRetries: 3,
  onRetry: (attempt, delayMs, reason) => {
    console.log(`Retry ${attempt}: ${reason} (waiting ${delayMs}ms)`);
  },
});

const api = new EventsApi(config);

async function fetchEventsSafely() {
  try {
    const response = await api.getEvents({
      page: 1,
      limit: 100,
      exactCount: false,
    });
    console.log(`Successfully fetched ${response.data.length} events`);
    return response;
  } catch (error) {
    console.error("Failed to fetch events after retries:", error);
    throw error;
  }
}

fetchEventsSafely();
```

### Example 2: Streaming with Resilience

```typescript
const config = new Configuration({
  basePath: "https://api.sorobanpulse.com",
  apiKey: "your-api-key",
});

const api = new EventsApi(config);

const stream = api.streamEventsSSE({
  onMessage: (event) => {
    try {
      const data = JSON.parse(event.data);
      console.log("Event:", data);
    } catch (e) {
      console.error("Failed to parse event:", e);
    }
  },
  onError: (error) => {
    console.error("Stream error:", error);
    // SSE automatically reconnects with backoff
  },
  autoReconnect: true,
  maxReconnectAttempts: 10,
});

stream.connect();
```

### Example 3: Custom Retry Strategy

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 2,
  retryInitialDelayMs: 100,  // Start with 100ms
  retryMaxDelayMs: 1000,     // Cap at 1 second
  retryOnStatus: [429, 503], // Selective retries
  onRetry: (attempt, delayMs, reason) => {
    console.info(
      `[Retry ${attempt}/2] ${reason} - waiting ${(delayMs / 1000).toFixed(2)}s`
    );
  },
});
```

## Migration from Old SDK

If you were using an older version, the retry configuration is now simpler:

### Old Way
```typescript
// Some SDKs required manual retry logic
for (let i = 0; i < 3; i++) {
  try {
    return await api.call();
  } catch (e) {
    if (i < 2) await sleep(Math.pow(2, i) * 1000);
  }
}
```

### New Way
```typescript
// Just configure and let the SDK handle it
const config = new Configuration({
  basePath: "...",
  maxRetries: 3,
});

return await api.call();  // Retries automatically
```

## Troubleshooting

### Q: Retries aren't happening
- Check that `maxRetries > 0`
- Verify the response status code is in `retryOnStatus`
- Check the `onRetry` callback is being called

### Q: Retries are taking too long
- Reduce `maxRetries`
- Reduce `retryInitialDelayMs`
- Reduce `retryMaxDelayMs`

### Q: Getting "failed after retries" errors
- Increase `maxRetries`
- Check server health
- Verify API key if using authentication

## See Also

- [Main TypeScript SDK README](./README.md)
- [Soroban Pulse API Documentation](https://soroban-pulse.com/docs)
- [HTTP Retry Strategies Guide](https://en.wikipedia.org/wiki/Exponential_backoff)
