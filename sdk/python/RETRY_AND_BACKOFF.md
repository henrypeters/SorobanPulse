# Retry Policy and Exponential Backoff in Soroban Pulse Python SDK

This document explains how to configure and use the retry and backoff functionality in the Soroban Pulse Python SDK.

## Overview

The SDK implements automatic retry with exponential backoff for transient failures, including:
- Network timeouts and connection errors
- HTTP 429 (Rate Limit) responses
- HTTP 5xx server errors (500, 502, 503, 504)

Retries are automatically applied with exponential backoff: 1s, 2s, 4s, 8s, 16s (default).

## Configuration

### Basic Setup with Defaults

```python
from openapi_client import ApiClient, Configuration

config = Configuration(
    host="https://api.sorobanpulse.com",
)
api_client = ApiClient(configuration=config)

# Retry configuration with defaults:
# max_retries: 3
# retry_on_status: [429, 500, 502, 503, 504]
# retry_initial_delay_ms: 1000 (1 second)
# retry_max_delay_ms: 32000 (32 seconds)
```

### Custom Retry Configuration

```python
from openapi_client import ApiClient, Configuration
from openapi_client import RetryPolicyConfig

config = Configuration(
    host="https://api.sorobanpulse.com",
)

retry_config = RetryPolicyConfig(
    max_retries=5,                          # Retry up to 5 times
    retryable_status_codes={429, 503, 504}, # Only retry these
    initial_delay_ms=500,                   # Start with 500ms
    max_delay_ms=60000,                     # Cap at 60s
    on_retry=lambda attempt, delay, reason: 
        print(f"Retry {attempt} after {delay}ms: {reason}")
)

api_client = ApiClient(configuration=config)
api_client.retry_policy = retry_policy
```

## Retry Policies

### 1. Default Retry Policy (Balanced)

Best for most use cases:

```python
from openapi_client import create_default_retry_policy

policy = create_default_retry_policy()
# max_retries: 3
# initial_delay_ms: 1000
# max_delay_ms: 32000
# retryable_status_codes: {429, 500, 502, 503, 504}
```

**Backoff sequence:** 1s, 2s, 4s, 8s, 16s

### 2. Aggressive Retry Policy

For critical operations that must succeed:

```python
from openapi_client import create_aggressive_retry_policy

policy = create_aggressive_retry_policy()
# max_retries: 5
# initial_delay_ms: 500
# max_delay_ms: 60000
# retryable_status_codes: {429, 500, 502, 503, 504}
```

**Backoff sequence:** 500ms, 1s, 2s, 4s, 8s, 16s, 32s, 60s

### 3. Conservative Retry Policy

For operations that should fail fast:

```python
from openapi_client import create_conservative_retry_policy

policy = create_conservative_retry_policy()
# max_retries: 1
# initial_delay_ms: 2000
# max_delay_ms: 5000
# retryable_status_codes: {503}  # Only service unavailable
```

**Backoff sequence:** 2s, 5s

## Monitoring Retries

### Retry Callback

Use the `on_retry` callback to monitor and log retry attempts:

```python
from openapi_client import RetryPolicyConfig, get_global_retry_policy

def log_retry(attempt, delay, reason):
    print(f"Attempt {attempt} failed: {reason}. Retrying after {delay}ms...")

config = RetryPolicyConfig(
    on_retry=log_retry
)

policy = get_global_retry_policy(config)
```

### Example with Request Tracking

```python
import asyncio
from openapi_client import ApiClient, Configuration, RetryPolicyConfig

class RequestMetrics:
    def __init__(self):
        self.total = 0
        self.retried = 0
        self.failed = 0

metrics = RequestMetrics()

def track_retry(attempt, delay, reason):
    metrics.retried += 1
    print(f"Retry {attempt} (total retries: {metrics.retried}): {reason}")

config = Configuration(host="https://api.sorobanpulse.com")
retry_config = RetryPolicyConfig(on_retry=track_retry)

api_client = ApiClient(configuration=config)
# Set retry policy on client

try:
    metrics.total += 1
    # Make API call
except Exception as e:
    metrics.failed += 1
    print(f"Request failed: {e}")

print(f"Metrics - Total: {metrics.total}, Retried: {metrics.retried}, Failed: {metrics.failed}")
```

## Advanced Usage

### Custom Retry Decision

Create a custom retry policy with custom logic:

```python
from openapi_client import RetryPolicy, exponential_backoff

class CustomRetryPolicy(RetryPolicy):
    def should_retry(self, error, attempt):
        # Custom logic: only retry on specific conditions
        if isinstance(error, ConnectionError):
            return attempt < self.config.max_retries
        if hasattr(error, 'status_code'):
            return error.status_code in self.config.retryable_status_codes
        return False

policy = CustomRetryPolicy()
```

### Retry-After Header Support

The SDK automatically respects the `Retry-After` header from the server:

```python
# Server responds with:
# HTTP 429
# Retry-After: 60
#
# The SDK will wait 60 seconds before retrying (instead of using exponential backoff)
```

### Backoff Strategies

The SDK supports different backoff strategies:

```python
from openapi_client import (
    exponential_backoff,
    linear_backoff,
    immediate_retry,
    RetryPolicyConfig
)

# Exponential backoff (default): 2^attempt * baseDelay
config1 = RetryPolicyConfig(
    backoff_strategy=exponential_backoff
)

# Linear backoff: attempt * baseDelay
config2 = RetryPolicyConfig(
    backoff_strategy=linear_backoff
)

# Immediate retry with minimal jitter
config3 = RetryPolicyConfig(
    backoff_strategy=immediate_retry
)
```

## Error Scenarios

### Rate Limiting (HTTP 429)

```python
from openapi_client import RetryPolicyConfig

config = RetryPolicyConfig(
    max_retries=3,
    retryable_status_codes={429},  # Retry on rate limit
    on_retry=lambda attempt, delay, reason: 
        print(f"Rate limited! Retrying after {delay}ms")
)

# The SDK will automatically retry with exponential backoff
```

### Server Errors (5xx)

```python
config = RetryPolicyConfig(
    max_retries=5,  # More retries for server errors
    retryable_status_codes={500, 502, 503, 504},
    on_retry=lambda attempt, delay, reason:
        print(f"Server error ({reason}). Retrying...")
)
```

### Network Timeouts

```python
from openapi_client import RetryPolicyConfig

config = RetryPolicyConfig(
    retryable_errors={ConnectionError, TimeoutError, OSError},
    on_retry=lambda attempt, delay, reason:
        print(f"Connection error. Retrying...")
)
```

## Async Usage

The Python SDK is async-compatible:

```python
import asyncio
from openapi_client import ApiClient, Configuration

async def fetch_events():
    config = Configuration(host="https://api.sorobanpulse.com")
    api_client = ApiClient(configuration=config)
    
    # API calls with automatic retries
    # events = await api_client.get_events(page=1, limit=20)
    
    return events

# Run async
asyncio.run(fetch_events())
```

## Metrics and Logging

### Get Retry Metrics

```python
from openapi_client import get_global_retry_policy

policy = get_global_retry_policy()

# Get aggregated metrics
metrics = policy.get_metrics()
print(f"Total attempts: {metrics.total_attempts}")
print(f"Total retries: {metrics.total_retries}")
print(f"Total delay: {metrics.total_delay_ms}ms")

# Get metrics for specific request
request_metrics = policy.get_metrics("GET:/v1/events")
print(f"Request retries: {request_metrics.total_retries}")
```

### Clear Metrics

```python
policy = get_global_retry_policy()
policy.clear_metrics()
```

## Best Practices

1. **Use appropriate retry counts:** More retries = longer waits. Balance for your use case.

2. **Monitor retry metrics:** Log retry attempts to understand server health and adjust configuration.

3. **Set reasonable max delays:** Very long delays may timeout at the application level.

4. **Respect Retry-After headers:** The SDK does this automatically; don't override.

5. **Implement circuit breaker:** For production, consider adding a circuit breaker pattern:

```python
class CircuitBreaker:
    def __init__(self, failure_threshold=5):
        self.failure_threshold = failure_threshold
        self.failure_count = 0
        self.is_open = False
    
    def record_retry(self):
        self.failure_count += 1
        if self.failure_count >= self.failure_threshold:
            self.is_open = True
    
    def can_proceed(self):
        return not self.is_open

circuit_breaker = CircuitBreaker()

def track_retry(attempt, delay, reason):
    circuit_breaker.record_retry()
    if circuit_breaker.is_open:
        raise Exception("Circuit breaker is open!")
```

## Examples

### Example 1: Fetch Events with Retry

```python
from openapi_client import (
    ApiClient,
    Configuration,
    EventsApi,
    RetryPolicyConfig,
)

config = Configuration(
    host="https://api.sorobanpulse.com",
    api_key="your-api-key"
)

retry_config = RetryPolicyConfig(
    max_retries=3,
    on_retry=lambda attempt, delay, reason:
        print(f"Retry {attempt}: {reason} (waiting {delay}ms)")
)

api_client = ApiClient(configuration=config)
# api_client.retry_policy = retry_policy

events_api = EventsApi(api_client)

try:
    response = events_api.get_events(page=1, limit=100)
    print(f"Successfully fetched {len(response.data)} events")
except Exception as error:
    print(f"Failed to fetch events: {error}")
```

### Example 2: Stream with Resilience

```python
from openapi_client import (
    ApiClient,
    Configuration,
    EventsApi,
    RetryPolicyConfig,
)

config = Configuration(
    host="https://api.sorobanpulse.com",
    api_key="your-api-key",
)

retry_config = RetryPolicyConfig(max_retries=5)

api_client = ApiClient(configuration=config)

events_api = EventsApi(api_client)

# The underlying SSE connection will use retry logic
# stream = events_api.stream_events(
#     on_message=lambda event: print(f"Event: {event}"),
#     auto_reconnect=True,
# )
```

### Example 3: Custom Retry Strategy

```python
from openapi_client import RetryPolicyConfig, exponential_backoff

config = RetryPolicyConfig(
    max_retries=2,
    initial_delay_ms=100,
    max_delay_ms=1000,
    retryable_status_codes={429, 503},
    backoff_strategy=exponential_backoff,
    on_retry=lambda attempt, delay, reason:
        print(f"[Retry {attempt}/2] {reason} - waiting {delay/1000:.2f}s")
)
```

## Migration from Old SDK

If you were using an older version, the retry configuration is now simpler:

### Old Way
```python
# Some SDKs required manual retry logic
import time
import random

for i in range(3):
    try:
        return api.call()
    except Exception:
        if i < 2:
            time.sleep((2 ** i) + random.random())
```

### New Way
```python
# Just configure and let the SDK handle it
config = RetryPolicyConfig(max_retries=3)
policy = get_global_retry_policy(config)

return api.call()  # Retries automatically
```

## Troubleshooting

### Q: Retries aren't happening
- Check that `max_retries > 0`
- Verify the response status code is in `retryable_status_codes`
- Check the `on_retry` callback is being called
- Verify the API client is using the retry policy

### Q: Retries are taking too long
- Reduce `max_retries`
- Reduce `initial_delay_ms`
- Reduce `max_delay_ms`

### Q: Getting "failed after retries" errors
- Increase `max_retries`
- Check server health
- Verify API key if using authentication

## See Also

- [Main Python SDK README](./README.md)
- [Soroban Pulse API Documentation](https://soroban-pulse.com/docs)
- [HTTP Retry Strategies Guide](https://en.wikipedia.org/wiki/Exponential_backoff)
- [Python asyncio Documentation](https://docs.python.org/3/library/asyncio.html)
