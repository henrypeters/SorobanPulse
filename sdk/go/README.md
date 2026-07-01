# Soroban Pulse Go Client SDK

A type-safe Go client library for consuming the Soroban Pulse API. Indexes Soroban smart contract events on the Stellar network.

## Features

- 🚀 Fast and efficient HTTP client with connection pooling
- 🔄 Automatic retry with exponential backoff (1s, 2s, 4s, 8s, 16s)
- 🎯 Context-aware API methods (full support for cancellation and timeouts)
- 🌊 Server-Sent Events (SSE) streaming support
- 📦 Zero external dependencies (uses only standard library)
- 🧪 Comprehensive examples and documentation

## Installation

```bash
go get github.com/soroban-pulse/client-go
```

## Quick Start

```go
package main

import (
	"context"
	"fmt"
	"log"
	
	sp "github.com/soroban-pulse/client-go"
)

func main() {
	client := sp.NewClient(sp.ClientConfig{
		BaseURL: "https://api.sorobanpulse.com",
		APIKey:  "your-api-key", // optional
	})
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	events, err := client.GetEvents(ctx, sp.NewGetEventsOptions())
	if err != nil {
		log.Fatal(err)
	}

	for _, event := range events.Data {
		fmt.Printf("Event %s: %s at ledger %d\n", event.ID, event.EventType, event.Ledger)
	}
}
```

## Configuration

### Basic Configuration

```go
client := sp.NewClient(sp.ClientConfig{
	BaseURL: "https://api.sorobanpulse.com",
	APIKey:  "your-api-key",
	Timeout: 30 * time.Second,
})
```

### Retry Configuration

```go
client := sp.NewClient(sp.ClientConfig{
	BaseURL:               "https://api.sorobanpulse.com",
	MaxRetries:           3,                    // Retry up to 3 times
	RetryInitialDelay:    1 * time.Second,      // 1 second initial
	RetryMaxDelay:        32 * time.Second,     // 32 seconds max
	RetryableStatusCodes: []int{429, 500, 502, 503, 504},
	OnRetry: func(attempt int, delay time.Duration, reason string) {
		log.Printf("Retry attempt %d: %s (delay: %v)", attempt, reason, delay)
	},
})
```

## Usage Examples

### Get Events

```go
ctx := context.Background()

// Get all events with pagination
events, err := client.GetEvents(ctx, &sp.GetEventsOptions{
	Page:  1,
	Limit: 50,
})
if err != nil {
	log.Fatal(err)
}

fmt.Printf("Found %d events\n", events.Total)
```

### Get Events by Contract

```go
contractID := "CAE2DPXVJ7JO7P3Q5I6H3L4M5N6O7P8Q9R0S1T2U3"

events, err := client.GetEventsByContract(ctx, contractID, &sp.GetEventsOptions{
	Limit: 100,
})
if err != nil {
	log.Fatal(err)
}

fmt.Printf("Found %d events for contract\n", len(events.Data))
```

### Get Events by Transaction Hash

```go
txHash := "abc123def456ghi789jkl012mno345pqr678stu901"

events, err := client.GetEventsByTransactionHash(ctx, txHash)
if err != nil {
	log.Fatal(err)
}

for _, event := range events.Data {
	fmt.Printf("Event: %v\n", event)
}
```

### Filter Events by Ledger Range

```go
events, err := client.GetEvents(ctx, &sp.GetEventsOptions{
	Page:       1,
	Limit:      50,
	FromLedger: 1000000,
	ToLedger:   1001000,
})
if err != nil {
	log.Fatal(err)
}
```

### Filter Events by Type

```go
events, err := client.GetEvents(ctx, &sp.GetEventsOptions{
	Page:      1,
	Limit:     50,
	EventType: "contract",  // "contract", "diagnostic", or "system"
})
if err != nil {
	log.Fatal(err)
}
```

### Stream Events with SSE

```go
handler := func(event *sp.Event) error {
	fmt.Printf("Received event: %s\n", event.ID)
	return nil
}

err := client.StreamEvents(ctx, nil, handler)
if err != nil {
	log.Fatal(err)
}
```

### Stream Events for Specific Contract

```go
contractID := "CAE2DPXVJ7JO7P3Q5I6H3L4M5N6O7P8Q9R0S1T2U3"

err := client.StreamEvents(ctx, &contractID, func(event *sp.Event) error {
	fmt.Printf("Event for contract: %s\n", event.ID)
	return nil
})
if err != nil {
	log.Fatal(err)
}
```

### Check Service Health

```go
health, err := client.GetHealth(ctx)
if err != nil {
	log.Fatal(err)
}

fmt.Printf("Service Status: %s\n", health.Status)
fmt.Printf("Database: %s\n", health.Database)
fmt.Printf("Indexer: %s\n", health.Indexer)
```

## Retry Policies

### Default Retry Policy

Best for most use cases. Retries up to 3 times with exponential backoff.

```go
client := sp.NewClient(sp.ClientConfig{
	BaseURL: "https://api.sorobanpulse.com",
	// Uses default retry policy
})
```

**Backoff sequence:** 1s, 2s, 4s, 8s, 16s

### Aggressive Retry Policy

For critical operations that must succeed.

```go
client := sp.NewClient(sp.ClientConfig{
	BaseURL:            "https://api.sorobanpulse.com",
	MaxRetries:         5,
	RetryInitialDelay:  500 * time.Millisecond,
	RetryMaxDelay:      60 * time.Second,
})
```

**Backoff sequence:** 500ms, 1s, 2s, 4s, 8s, 16s, 32s, 60s

### Conservative Retry Policy

For operations that should fail fast.

```go
client := sp.NewClient(sp.ClientConfig{
	BaseURL:               "https://api.sorobanpulse.com",
	MaxRetries:           1,
	RetryInitialDelay:    2 * time.Second,
	RetryMaxDelay:        5 * time.Second,
	RetryableStatusCodes: []int{503}, // Only retry service unavailable
})
```

## Context Usage

All API methods support Go's context for cancellation and timeouts.

### Timeout

```go
ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
defer cancel()

events, err := client.GetEvents(ctx, sp.NewGetEventsOptions())
```

### Cancellation

```go
ctx, cancel := context.WithCancel(context.Background())
go func() {
	time.Sleep(5 * time.Second)
	cancel()
}()

err := client.StreamEvents(ctx, nil, func(event *sp.Event) error {
	fmt.Printf("Event: %s\n", event.ID)
	return nil
})
// Will return context.Canceled after 5 seconds
```

## Error Handling

```go
events, err := client.GetEvents(ctx, sp.NewGetEventsOptions())
if err != nil {
	// Handle different error types
	if err == context.DeadlineExceeded {
		log.Println("Request timeout")
	} else if err == context.Canceled {
		log.Println("Request was cancelled")
	} else {
		log.Printf("API error: %v", err)
	}
}
```

## Connection Pooling

The client automatically manages connection pooling through the `http.Client`.

```go
client := sp.NewClient(sp.ClientConfig{
	BaseURL: "https://api.sorobanpulse.com",
	Timeout: 30 * time.Second,
})

// Make multiple requests - connections are reused
for i := 0; i < 100; i++ {
	events, _ := client.GetEvents(ctx, sp.NewGetEventsOptions())
	process(events)
}
```

## Monitoring and Logging

### Retry Monitoring

```go
client := sp.NewClient(sp.ClientConfig{
	BaseURL: "https://api.sorobanpulse.com",
	OnRetry: func(attempt int, delay time.Duration, reason string) {
		log.Printf(
			"[Retry %d] %s - waiting %v",
			attempt,
			reason,
			delay,
		)
	},
})
```

### Request Tracking

```go
type RequestMetrics struct {
	Total    int
	Retried  int
	Failed   int
}

metrics := &RequestMetrics{}

client := sp.NewClient(sp.ClientConfig{
	BaseURL: "https://api.sorobanpulse.com",
	OnRetry: func(attempt int, delay time.Duration, reason string) {
		metrics.Retried++
	},
})

events, err := client.GetEvents(ctx, sp.NewGetEventsOptions())
if err != nil {
	metrics.Failed++
} else {
	metrics.Total++
}

fmt.Printf("Metrics - Total: %d, Retried: %d, Failed: %d\n",
	metrics.Total, metrics.Retried, metrics.Failed)
```

## Best Practices

1. **Reuse Client Instances**
   ```go
   // Good - reuse across requests
   client := sp.NewClient(config)
   for range requests {
       client.GetEvents(ctx, opts)
   }
   ```

2. **Use Context Properly**
   ```go
   // Good - context with timeout
   ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
   defer cancel()
   ```

3. **Handle Errors**
   ```go
   // Good - check errors
   events, err := client.GetEvents(ctx, opts)
   if err != nil {
       log.Fatalf("Failed to get events: %v", err)
   }
   ```

4. **Close Client When Done**
   ```go
   // Good - clean up resources
   defer client.Close()
   ```

5. **Configure Retry Policy**
   ```go
   // Good - adjust for your use case
   client := sp.NewClient(sp.ClientConfig{
       MaxRetries:        3,
       RetryMaxDelay:     32 * time.Second,
   })
   ```

## Troubleshooting

### Q: Retries aren't happening
- Check that `MaxRetries > 0`
- Verify the response status code is in `RetryableStatusCodes`
- Check the `OnRetry` callback is being called

### Q: Getting timeout errors
- Increase the `Timeout` duration
- Check network connectivity
- Verify the API endpoint is accessible

### Q: SSL certificate errors
- Ensure your system's CA certificates are up to date
- Check if you're behind a proxy that intercepts HTTPS

## Contributing

Contributions are welcome! Please see our [Contributing Guide](../../CONTRIBUTING.md) for details.

## License

This SDK is licensed under the same license as the Soroban Pulse project.

## Resources

- [Soroban Pulse API Documentation](https://soroban-pulse.com/docs)
- [Stellar Network Documentation](https://developers.stellar.org/soroban)
- [Go Context Best Practices](https://pkg.go.dev/context)
