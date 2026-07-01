/**
 * Soroban Pulse TypeScript SDK - Usage Examples
 * 
 * This file demonstrates common usage patterns for the SDK including:
 * - Basic event queries
 * - Retry and backoff configuration
 * - SSE streaming
 * - Interceptors
 */

import {
  EventsApi,
  SystemApi,
  Configuration,
} from "./index";

// ============================================================================
// Example 1: Basic Event Queries
// ============================================================================

async function basicEventQuery() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key", // optional
  });

  const api = new EventsApi(config);

  try {
    // Get events with pagination
    const response = await api.getEvents({
      page: 1,
      limit: 20,
      exactCount: false,
    });

    console.log(`Total events: ${response.total}`);
    console.log(`Events on this page: ${response.data.length}`);

    response.data.forEach((event) => {
      console.log(`Event ${event.id} at ledger ${event.ledger}`);
    });
  } catch (error) {
    console.error("Failed to fetch events:", error);
  }
}

// ============================================================================
// Example 2: Retry and Exponential Backoff Configuration
// ============================================================================

async function withRetryConfiguration() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    
    // Retry configuration
    maxRetries: 3,                    // Retry up to 3 times
    retryOnStatus: [429, 500, 502, 503, 504],  // These status codes trigger retry
    retryInitialDelayMs: 1000,        // Start with 1 second
    retryMaxDelayMs: 32000,           // Max wait is 32 seconds
    
    // Callback when retry happens
    onRetry: (attempt, delayMs, reason) => {
      console.log(
        `Retry attempt ${attempt}: ${reason} ` +
        `(waiting ${(delayMs / 1000).toFixed(1)}s)`
      );
    },
  });

  const api = new EventsApi(config);

  try {
    const response = await api.getEvents({ page: 1, limit: 100 });
    console.log(`Successfully fetched ${response.data.length} events`);
  } catch (error) {
    console.error("Failed after all retries:", error);
  }
}

// ============================================================================
// Example 3: Aggressive Retry Policy
// ============================================================================

async function aggressiveRetryPolicy() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    
    // For critical operations - retry more aggressively
    maxRetries: 5,                    // 5 retries
    retryInitialDelayMs: 500,         // 500ms initial
    retryMaxDelayMs: 60000,           // Up to 60 seconds
    
    onRetry: (attempt, delayMs, reason) => {
      console.warn(
        `Critical operation retry ${attempt}/5: ${reason} ` +
        `(delay: ${(delayMs / 1000).toFixed(1)}s)`
      );
    },
  });

  const api = new EventsApi(config);
  const contractId = "CAE2DPXVJ7JO7P3Q5I6H3L4M5N6O7P8Q9R0S1T2U3";

  try {
    const response = await api.getEventsByContract({
      contractId,
      page: 1,
      limit: 50,
    });
    console.log(
      `Found ${response.data.length} events for contract ${contractId}`
    );
  } catch (error) {
    console.error("Operation failed despite aggressive retries:", error);
  }
}

// ============================================================================
// Example 4: Conservative Retry Policy
// ============================================================================

async function conservativeRetryPolicy() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    
    // For operations that should fail fast
    maxRetries: 1,                    // Only retry once
    retryInitialDelayMs: 2000,        // 2 seconds
    retryMaxDelayMs: 5000,            // Max 5 seconds
    retryOnStatus: [503],             // Only retry service unavailable
    
    onRetry: (attempt, delayMs, reason) => {
      console.warn(`Quick retry: ${reason}`);
    },
  });

  const api = new EventsApi(config);

  try {
    const response = await api.getEvents({ page: 1, limit: 10 });
    return response;
  } catch (error) {
    console.error("Quick operation failed:", error);
    throw error;
  }
}

// ============================================================================
// Example 5: SSE Streaming with Retry
// ============================================================================

async function streamEventsWithRetry() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    maxRetries: 3,
    retryInitialDelayMs: 1000,
  });

  const api = new EventsApi(config);

  // Stream all events
  const stream = api.streamEventsSSE({
    onMessage: (event) => {
      try {
        const eventData = JSON.parse(event.data);
        console.log("New event:", eventData);
      } catch (e) {
        console.error("Failed to parse event:", e);
      }
    },
    
    onPing: (timestamp) => {
      console.debug("Server ping at", new Date(timestamp).toISOString());
    },
    
    onClose: () => {
      console.log("Stream closed by server");
      // Will automatically reconnect
    },
    
    onError: (error) => {
      console.error("Stream error:", error);
      // Automatic reconnection will happen
    },
    
    // Streaming configuration
    autoReconnect: true,
    maxReconnectAttempts: 10,
    reconnectDelayMs: 1000,
  });

  stream.connect();

  // Keep stream open for 1 minute, then close
  setTimeout(() => {
    stream.disconnect();
  }, 60000);
}

// ============================================================================
// Example 6: Stream Events for Specific Contract
// ============================================================================

async function streamContractEvents() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    maxRetries: 3,
  });

  const api = new EventsApi(config);
  const contractId = "CAE2DPXVJ7JO7P3Q5I6H3L4M5N6O7P8Q9R0S1T2U3";

  const stream = api.streamEventsByContractSSE(contractId, {
    onMessage: (event) => {
      const eventData = JSON.parse(event.data);
      console.log(`Event for contract ${contractId}:`, eventData);
    },
    autoReconnect: true,
  });

  stream.connect();
}

// ============================================================================
// Example 7: Stream Multiple Contracts
// ============================================================================

async function streamMultipleContracts() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    maxRetries: 3,
  });

  const api = new EventsApi(config);
  const contractIds = [
    "CABC1234567890ABCDEF1234567890ABCDEF1234",
    "CDEF5678901234ABCDEF5678901234ABCDEF5678",
    "C1234ABCDEF5678901234ABCDEF5678901234ABCD",
  ];

  const stream = api.streamMultiEventsSSE(contractIds, {
    onMessage: (event) => {
      const eventData = JSON.parse(event.data);
      console.log(
        "Event from one of the contracts:",
        eventData.contract_id,
        eventData
      );
    },
    autoReconnect: true,
  });

  stream.connect();
}

// ============================================================================
// Example 8: Get Events by Transaction Hash
// ============================================================================

async function eventsByTransactionHash() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    maxRetries: 3,
  });

  const api = new EventsApi(config);
  const txHash = "abc123def456ghi789jkl012mno345pqr678stu901";

  try {
    const response = await api.getEventsByTransactionHash({
      txHash,
    });
    console.log(
      `Found ${response.data.length} events for transaction ${txHash}`
    );
    response.data.forEach((event) => {
      console.log(`  - Event ${event.id}: ${event.event_type}`);
    });
  } catch (error) {
    console.error("Failed to fetch events by tx hash:", error);
  }
}

// ============================================================================
// Example 9: Ledger Range Filtering
// ============================================================================

async function eventsByLedgerRange() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    maxRetries: 3,
  });

  const api = new EventsApi(config);

  try {
    const response = await api.getEvents({
      page: 1,
      limit: 50,
      fromLedger: 1000000,  // Start ledger
      toLedger: 1001000,    // End ledger
      exactCount: true,     // Get exact count (slower but accurate)
    });
    console.log(
      `Found ${response.data.length} events ` +
      `(total: ${response.total}) between ledgers 1000000-1001000`
    );
  } catch (error) {
    console.error("Failed to fetch events by ledger range:", error);
  }
}

// ============================================================================
// Example 10: Event Type Filtering
// ============================================================================

async function eventsByType() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    maxRetries: 3,
  });

  const api = new EventsApi(config);

  try {
    // Get only contract events
    const response = await api.getEvents({
      page: 1,
      limit: 50,
      eventType: "contract",  // 'contract', 'diagnostic', or 'system'
    });
    console.log(`Found ${response.data.length} contract events`);
  } catch (error) {
    console.error("Failed to fetch contract events:", error);
  }
}

// ============================================================================
// Example 11: Health Check
// ============================================================================

async function checkServiceHealth() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
  });

  const api = new SystemApi(config);

  try {
    const health = await api.getHealthz();
    console.log("Service health:", health);
    console.log("Database:", health.db);
    console.log("Indexer:", health.indexer);
  } catch (error) {
    console.error("Service is unhealthy:", error);
  }
}

// ============================================================================
// Example 12: Error Handling with Retry Metrics
// ============================================================================

async function errorHandlingWithMetrics() {
  let retryCount = 0;
  let totalWaitTime = 0;

  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",
    maxRetries: 3,
    retryInitialDelayMs: 1000,
    onRetry: (attempt, delayMs, reason) => {
      retryCount++;
      totalWaitTime += delayMs;
      console.log(
        `[${new Date().toISOString()}] Retry ${attempt}: ${reason} ` +
        `(delay: ${delayMs}ms, total wait: ${totalWaitTime}ms)`
      );
    },
  });

  const api = new EventsApi(config);

  try {
    const response = await api.getEvents({
      page: 1,
      limit: 100,
    });
    console.log(
      `Success! Fetched ${response.data.length} events ` +
      `(retries: ${retryCount}, total wait: ${totalWaitTime}ms)`
    );
  } catch (error) {
    console.error(
      `Failed after ${retryCount} retries ` +
      `(total wait: ${totalWaitTime}ms): ${error.message}`
    );
  }
}

// ============================================================================
// Example 13: NDJSON Response Format
// ============================================================================

async function exportEventsAsNDJSON() {
  const config = new Configuration({
    basePath: "https://api.sorobanpulse.com",
    apiKey: "your-api-key",  // Required for export endpoint
    maxRetries: 3,
  });

  const api = new EventsApi(config);

  try {
    // Request NDJSON format (one JSON object per line)
    const response = await api.getEventsExport({
      // This would be handled by Accept header in the actual API
    });
    
    // Process events as they stream in
    const lines = response.split("\n");
    lines.forEach((line) => {
      if (line.trim()) {
        const event = JSON.parse(line);
        console.log("Event:", event);
      }
    });
  } catch (error) {
    console.error("Failed to export events:", error);
  }
}

// ============================================================================
// Run Examples
// ============================================================================

async function runAllExamples() {
  console.log("Example 1: Basic Event Query");
  await basicEventQuery().catch(console.error);

  console.log("\nExample 2: With Retry Configuration");
  await withRetryConfiguration().catch(console.error);

  console.log("\nExample 3: Aggressive Retry Policy");
  await aggressiveRetryPolicy().catch(console.error);

  console.log("\nExample 4: Conservative Retry Policy");
  await conservativeRetryPolicy().catch(console.error);

  console.log("\nExample 11: Health Check");
  await checkServiceHealth().catch(console.error);

  console.log("\nExample 12: Error Handling with Metrics");
  await errorHandlingWithMetrics().catch(console.error);
}

// Export examples for use in documentation
export {
  basicEventQuery,
  withRetryConfiguration,
  aggressiveRetryPolicy,
  conservativeRetryPolicy,
  streamEventsWithRetry,
  streamContractEvents,
  streamMultipleContracts,
  eventsByTransactionHash,
  eventsByLedgerRange,
  eventsByType,
  checkServiceHealth,
  errorHandlingWithMetrics,
  exportEventsAsNDJSON,
  runAllExamples,
};
