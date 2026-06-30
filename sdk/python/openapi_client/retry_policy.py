"""
Retry policy configuration and implementation for Soroban Pulse SDK

Features:
- Configurable retry policies
- Exponential backoff with jitter
- Custom retry strategies
- Retry metrics and logging
"""

import time
import random
import logging
from typing import Callable, Optional, Set, Type, Dict, Any
from dataclasses import dataclass, field
from datetime import datetime

logger = logging.getLogger(__name__)


@dataclass
class RetryMetrics:
    """Track retry behavior for requests"""
    total_attempts: int = 0
    total_retries: int = 0
    last_retry_time: Optional[datetime] = None
    total_delay_ms: float = 0
    success_on_attempt: Optional[int] = None


# Type definitions
RetryDecision = Callable[[Exception, int, RetryMetrics], bool]
BackoffStrategy = Callable[[int, Optional[int]], float]


def exponential_backoff(attempt: int, base_delay_ms: int = 1000) -> float:
    """
    Exponential backoff strategy: 1s, 2s, 4s, 8s, 16s
    
    Args:
        attempt: The current retry attempt number (0-indexed)
        base_delay_ms: Base delay in milliseconds
        
    Returns:
        Delay in milliseconds with jitter
    """
    exponential_delay = (2 ** attempt) * base_delay_ms
    jitter = random.random() * base_delay_ms
    return (exponential_delay + jitter) / 1000.0  # Convert to seconds


def linear_backoff(attempt: int, base_delay_ms: int = 1000) -> float:
    """
    Linear backoff strategy: delay * attempt
    
    Args:
        attempt: The current retry attempt number
        base_delay_ms: Base delay in milliseconds
        
    Returns:
        Delay in seconds with jitter
    """
    linear_delay = attempt * base_delay_ms
    jitter = random.random() * base_delay_ms
    return (linear_delay + jitter) / 1000.0


def immediate_retry(attempt: int, base_delay_ms: int = 100) -> float:
    """
    Immediate retry with minimal jitter (risky, use with caution)
    
    Args:
        attempt: The current retry attempt number
        base_delay_ms: Maximum random jitter in milliseconds
        
    Returns:
        Delay in seconds (mostly jitter only)
    """
    return (random.random() * base_delay_ms) / 1000.0


@dataclass
class RetryPolicyConfig:
    """Retry policy configuration"""
    max_retries: int = 3
    initial_delay_ms: int = 1000
    max_delay_ms: int = 32000  # 32 seconds
    backoff_strategy: BackoffStrategy = field(default_factory=lambda: exponential_backoff)
    retryable_status_codes: Set[int] = field(default_factory=lambda: {429, 500, 502, 503, 504})
    retryable_errors: Set[Type[Exception]] = field(
        default_factory=lambda: {
            ConnectionError,
            TimeoutError,
            OSError,
        }
    )
    custom_retry_decision: Optional[RetryDecision] = None
    on_retry: Optional[Callable[[int, float, str], None]] = None


def create_default_retry_policy() -> RetryPolicyConfig:
    """Create a retry policy with sensible defaults"""
    return RetryPolicyConfig()


def create_aggressive_retry_policy() -> RetryPolicyConfig:
    """Create a retry policy for aggressive retry scenarios (many retries)"""
    return RetryPolicyConfig(
        max_retries=5,
        initial_delay_ms=500,
        max_delay_ms=60000,
        backoff_strategy=exponential_backoff,
        retryable_status_codes={429, 500, 502, 503, 504},
        retryable_errors={ConnectionError, TimeoutError, OSError},
    )


def create_conservative_retry_policy() -> RetryPolicyConfig:
    """Create a retry policy for conservative retry scenarios (minimal retries)"""
    return RetryPolicyConfig(
        max_retries=1,
        initial_delay_ms=2000,
        max_delay_ms=5000,
        backoff_strategy=linear_backoff,
        retryable_status_codes={503},  # Only retry on service unavailable
        retryable_errors={ConnectionError},
    )


class RetryPolicy:
    """Retry policy manager"""

    def __init__(self, config: Optional[RetryPolicyConfig] = None):
        self.config = config or create_default_retry_policy()
        self._metrics: Dict[str, RetryMetrics] = {}

    def should_retry(self, error: Exception, attempt: int) -> bool:
        """
        Check if an error should be retried
        
        Args:
            error: The exception or HTTP status code
            attempt: Current attempt number (0-indexed)
            
        Returns:
            True if should retry, False otherwise
        """
        if attempt >= self.config.max_retries:
            return False

        # Check custom retry decision first
        if self.config.custom_retry_decision:
            metrics = self.get_metrics()
            return self.config.custom_retry_decision(error, attempt, metrics)

        # Check HTTP status codes (if error has a status_code attribute)
        if hasattr(error, 'status_code'):
            return error.status_code in self.config.retryable_status_codes

        # Check error types
        for error_type in self.config.retryable_errors:
            if isinstance(error, error_type):
                return True

        return False

    def get_delay_seconds(self, attempt: int, retry_after_header: Optional[str] = None) -> float:
        """
        Calculate delay for retry attempt
        
        Args:
            attempt: Current attempt number
            retry_after_header: Value of Retry-After header if present
            
        Returns:
            Delay in seconds
        """
        # Check for Retry-After header first
        if retry_after_header:
            try:
                parsed = float(retry_after_header)
                return min(parsed, self.config.max_delay_ms / 1000.0)
            except ValueError:
                pass

        # Use backoff strategy
        delay = self.config.backoff_strategy(attempt, self.config.initial_delay_ms)
        return min(delay, self.config.max_delay_ms / 1000.0)

    def record_retry(
        self,
        request_key: str,
        attempt: int,
        delay_seconds: float,
        reason: str,
    ) -> None:
        """
        Record a retry attempt
        
        Args:
            request_key: Unique identifier for the request
            attempt: Current attempt number
            delay_seconds: Delay applied before retry
            reason: Reason for retry (e.g., 'HTTP 503')
        """
        if request_key not in self._metrics:
            self._metrics[request_key] = RetryMetrics()

        metrics = self._metrics[request_key]
        metrics.total_retries += 1
        metrics.total_delay_ms += delay_seconds * 1000
        metrics.last_retry_time = datetime.now()

        if self.config.on_retry:
            self.config.on_retry(attempt, delay_seconds, reason)

        logger.debug(f"Retry attempt {attempt + 1}/{self.config.max_retries + 1}: {reason} (delay: {delay_seconds:.2f}s)")

    def record_success(self, request_key: str, attempt: int) -> None:
        """
        Record a successful response
        
        Args:
            request_key: Unique identifier for the request
            attempt: Attempt number on which success occurred
        """
        if request_key not in self._metrics:
            self._metrics[request_key] = RetryMetrics()

        metrics = self._metrics[request_key]
        metrics.total_attempts = attempt + 1
        metrics.success_on_attempt = attempt

    def get_metrics(self, request_key: Optional[str] = None) -> RetryMetrics:
        """
        Get retry metrics for a request or aggregate metrics
        
        Args:
            request_key: Specific request key, or None for aggregate
            
        Returns:
            RetryMetrics object
        """
        if request_key:
            return self._metrics.get(request_key, RetryMetrics())

        # Aggregate all metrics
        aggregate = RetryMetrics()
        for metrics in self._metrics.values():
            aggregate.total_attempts += metrics.total_attempts
            aggregate.total_retries += metrics.total_retries
            aggregate.total_delay_ms += metrics.total_delay_ms
            if metrics.last_retry_time and (
                aggregate.last_retry_time is None
                or metrics.last_retry_time > aggregate.last_retry_time
            ):
                aggregate.last_retry_time = metrics.last_retry_time

        return aggregate

    def clear_metrics(self) -> None:
        """Clear all recorded metrics"""
        self._metrics.clear()

    def update_config(self, config: RetryPolicyConfig) -> None:
        """
        Update configuration
        
        Args:
            config: New retry policy configuration
        """
        self.config = config

    def get_config(self) -> RetryPolicyConfig:
        """Get current configuration"""
        return self.config


# Singleton instance for global retry policy
_global_retry_policy: Optional[RetryPolicy] = None


def get_global_retry_policy(
    config: Optional[RetryPolicyConfig] = None,
) -> RetryPolicy:
    """
    Get or create global retry policy
    
    Args:
        config: Initial configuration (only used on first call)
        
    Returns:
        Global RetryPolicy instance
    """
    global _global_retry_policy
    if _global_retry_policy is None:
        _global_retry_policy = RetryPolicy(config)
    return _global_retry_policy


def reset_global_retry_policy() -> None:
    """Reset global retry policy"""
    global _global_retry_policy
    _global_retry_policy = None
