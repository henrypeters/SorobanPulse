"""
Soroban Pulse Python SDK - Usage Examples

This module demonstrates common usage patterns for the SDK including:
- Basic event queries
- Retry and backoff configuration
- SSE streaming
- Error handling
"""

import asyncio
import logging
from typing import Optional

from openapi_client import (
    ApiClient,
    Configuration,
    EventsApi,
    SystemApi,
    RetryPolicyConfig,
    create_default_retry_policy,
    create_aggressive_retry_policy,
    create_conservative_retry_policy,
    get_global_retry_policy,
)

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


# ============================================================================
# Example 1: Basic Event Queries
# ============================================================================

def basic_event_query():
    """Get events with pagination"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",  # optional
    )

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        # Get events with pagination
        response = events_api.get_events(page=1, limit=20)
        logger.info(f"Total events: {response.total}")
        logger.info(f"Events on this page: {len(response.data)}")

        for event in response.data:
            logger.info(f"Event {event.id} at ledger {event.ledger}")
    except Exception as error:
        logger.error(f"Failed to fetch events: {error}")


# ============================================================================
# Example 2: Retry Configuration
# ============================================================================

def with_retry_configuration():
    """Configure retry and exponential backoff"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    # Create retry policy configuration
    retry_config = RetryPolicyConfig(
        max_retries=3,  # Retry up to 3 times
        initial_delay_ms=1000,  # Start with 1 second
        max_delay_ms=32000,  # Max wait is 32 seconds
        retryable_status_codes={429, 500, 502, 503, 504},
        on_retry=lambda attempt, delay, reason: logger.info(
            f"Retry attempt {attempt}: {reason} (waiting {delay}ms)"
        ),
    )

    # Get global retry policy and update with config
    retry_policy = get_global_retry_policy(retry_config)

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        response = events_api.get_events(page=1, limit=100)
        logger.info(f"Successfully fetched {len(response.data)} events")
    except Exception as error:
        logger.error(f"Failed after all retries: {error}")


# ============================================================================
# Example 3: Aggressive Retry Policy
# ============================================================================

def aggressive_retry_policy():
    """Use aggressive retry policy for critical operations"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    # Use pre-configured aggressive retry policy
    retry_policy = create_aggressive_retry_policy()
    retry_policy.on_retry = (
        lambda attempt, delay, reason: logger.warning(
            f"Critical operation retry {attempt}/5: {reason} (delay: {delay}ms)"
        )
    )

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    contract_id = "CAE2DPXVJ7JO7P3Q5I6H3L4M5N6O7P8Q9R0S1T2U3"

    try:
        response = events_api.get_events_by_contract(contract_id=contract_id)
        logger.info(f"Found {len(response.data)} events for contract")
    except Exception as error:
        logger.error(f"Operation failed despite aggressive retries: {error}")


# ============================================================================
# Example 4: Conservative Retry Policy
# ============================================================================

def conservative_retry_policy():
    """Use conservative retry policy for operations that should fail fast"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    # Use pre-configured conservative retry policy
    retry_policy = create_conservative_retry_policy()

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        response = events_api.get_events(page=1, limit=10)
        return response
    except Exception as error:
        logger.error(f"Quick operation failed: {error}")
        raise


# ============================================================================
# Example 5: Get Events by Contract
# ============================================================================

def get_events_by_contract():
    """Get events for a specific contract"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    contract_id = "CAE2DPXVJ7JO7P3Q5I6H3L4M5N6O7P8Q9R0S1T2U3"

    try:
        response = events_api.get_events_by_contract(
            contract_id=contract_id,
            page=1,
            limit=100,
        )
        logger.info(f"Found {len(response.data)} events for contract {contract_id}")

        for event in response.data:
            logger.info(f"  - Event {event.id}: {event.event_type}")
    except Exception as error:
        logger.error(f"Failed to fetch events by contract: {error}")


# ============================================================================
# Example 6: Get Events by Transaction Hash
# ============================================================================

def get_events_by_transaction_hash():
    """Get events for a specific transaction"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    tx_hash = "abc123def456ghi789jkl012mno345pqr678stu901"

    try:
        response = events_api.get_events_by_transaction_hash(tx_hash=tx_hash)
        logger.info(f"Found {len(response.data)} events for transaction {tx_hash}")

        for event in response.data:
            logger.info(f"  - Event {event.id}: {event.event_type}")
    except Exception as error:
        logger.error(f"Failed to fetch events by tx hash: {error}")


# ============================================================================
# Example 7: Filter Events by Ledger Range
# ============================================================================

def events_by_ledger_range():
    """Get events within a ledger range"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        response = events_api.get_events(
            page=1,
            limit=50,
            from_ledger=1000000,  # Start ledger
            to_ledger=1001000,    # End ledger
            exact_count=True,     # Get exact count
        )
        logger.info(
            f"Found {len(response.data)} events "
            f"(total: {response.total}) between ledgers 1000000-1001000"
        )
    except Exception as error:
        logger.error(f"Failed to fetch events by ledger range: {error}")


# ============================================================================
# Example 8: Filter Events by Type
# ============================================================================

def events_by_type():
    """Get events filtered by type"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        # Get only contract events
        response = events_api.get_events(
            page=1,
            limit=50,
            event_type="contract",  # 'contract', 'diagnostic', or 'system'
        )
        logger.info(f"Found {len(response.data)} contract events")
    except Exception as error:
        logger.error(f"Failed to fetch contract events: {error}")


# ============================================================================
# Example 9: Health Check
# ============================================================================

def check_service_health():
    """Check the service health status"""
    config = Configuration(host="https://api.sorobanpulse.com")

    api_client = ApiClient(configuration=config)
    system_api = SystemApi(api_client)

    try:
        health = system_api.get_healthz()
        logger.info(f"Service health: {health.status}")
        logger.info(f"Database: {health.db}")
        logger.info(f"Indexer: {health.indexer}")
    except Exception as error:
        logger.error(f"Service is unhealthy: {error}")


# ============================================================================
# Example 10: Error Handling with Metrics
# ============================================================================

def error_handling_with_metrics():
    """Handle errors and track retry metrics"""
    retry_count = 0
    total_wait_time = 0

    def track_retry(attempt, delay, reason):
        nonlocal retry_count, total_wait_time
        retry_count += 1
        total_wait_time += delay
        logger.info(
            f"Retry {attempt}: {reason} "
            f"(delay: {delay}ms, total wait: {total_wait_time}ms)"
        )

    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    retry_config = RetryPolicyConfig(on_retry=track_retry)
    get_global_retry_policy(retry_config)

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        response = events_api.get_events(page=1, limit=100)
        logger.info(
            f"Success! Fetched {len(response.data)} events "
            f"(retries: {retry_count}, total wait: {total_wait_time}ms)"
        )
    except Exception as error:
        logger.error(
            f"Failed after {retry_count} retries "
            f"(total wait: {total_wait_time}ms): {error}"
        )


# ============================================================================
# Example 11: Custom Retry Strategy
# ============================================================================

def custom_retry_strategy():
    """Configure a custom retry strategy"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    # Create custom retry policy
    retry_config = RetryPolicyConfig(
        max_retries=2,
        initial_delay_ms=100,
        max_delay_ms=1000,
        retryable_status_codes={429, 503},  # Selective retries
        on_retry=lambda attempt, delay, reason: logger.info(
            f"[Retry {attempt}/2] {reason} - waiting {delay / 1000:.2f}s"
        ),
    )

    get_global_retry_policy(retry_config)

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        response = events_api.get_events(page=1, limit=50)
        logger.info(f"Fetched {len(response.data)} events")
    except Exception as error:
        logger.error(f"Failed: {error}")


# ============================================================================
# Example 12: Async Usage
# ============================================================================

async def async_event_query():
    """Async event query"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    try:
        # Note: API calls might be async depending on implementation
        response = events_api.get_events(page=1, limit=20)
        logger.info(f"Async: Fetched {len(response.data)} events")
    except Exception as error:
        logger.error(f"Async query failed: {error}")


# ============================================================================
# Example 13: Multiple Sequential Calls
# ============================================================================

def multiple_sequential_calls():
    """Make multiple sequential API calls"""
    config = Configuration(
        host="https://api.sorobanpulse.com",
        api_key="your-api-key",
    )

    retry_config = RetryPolicyConfig(max_retries=3)
    get_global_retry_policy(retry_config)

    api_client = ApiClient(configuration=config)
    events_api = EventsApi(api_client)

    total_events = 0

    # Fetch first 5 pages
    for page in range(1, 6):
        try:
            response = events_api.get_events(page=page, limit=50)
            total_events += len(response.data)
            logger.info(f"Page {page}: fetched {len(response.data)} events")
        except Exception as error:
            logger.error(f"Page {page} failed: {error}")
            break

    logger.info(f"Total events across pages: {total_events}")


# ============================================================================
# Run Examples
# ============================================================================

def main():
    """Run all examples"""
    logger.info("Example 1: Basic Event Query")
    basic_event_query()

    logger.info("\nExample 2: With Retry Configuration")
    with_retry_configuration()

    logger.info("\nExample 3: Aggressive Retry Policy")
    aggressive_retry_policy()

    logger.info("\nExample 9: Health Check")
    check_service_health()

    logger.info("\nExample 10: Error Handling with Metrics")
    error_handling_with_metrics()


if __name__ == "__main__":
    main()
