/**
 * Retry policy configuration and implementation for Soroban Pulse SDK
 * 
 * Features:
 * - Configurable retry policies
 * - Exponential backoff with jitter
 * - Custom retry strategies
 * - Retry metrics and logging
 */

/**
 * Retry metrics to track retry behavior
 */
export interface RetryMetrics {
  totalAttempts: number;
  totalRetries: number;
  lastRetryTime?: Date;
  totalDelayMs: number;
  successOnAttempt?: number;
}

/**
 * Retry decision function - allows custom retry logic
 */
export type RetryDecision = (
  error: Error | Response,
  attempt: number,
  metrics: RetryMetrics
) => boolean;

/**
 * Backoff strategy function - calculates delay based on attempt number
 */
export type BackoffStrategy = (
  attempt: number,
  baseDelayMs?: number
) => number;

/**
 * Retry policy configuration
 */
export interface RetryPolicyConfig {
  maxRetries: number;
  initialDelayMs: number;
  maxDelayMs: number;
  backoffStrategy: BackoffStrategy;
  retryableStatusCodes: number[];
  retryableErrors: (new (...args: any[]) => Error)[];
  customRetryDecision?: RetryDecision;
  onRetry?: (attempt: number, delay: number, reason: string) => void;
}

/**
 * Default exponential backoff strategy: 1s, 2s, 4s, 8s, 16s
 */
export function exponentialBackoff(
  attempt: number,
  baseDelayMs: number = 1000
): number {
  // 2^attempt seconds with jitter
  const exponentialDelay = Math.pow(2, attempt) * baseDelayMs;
  const jitter = Math.random() * baseDelayMs; // Add random jitter
  return exponentialDelay + jitter;
}

/**
 * Linear backoff strategy: delay * attempt
 */
export function linearBackoff(
  attempt: number,
  baseDelayMs: number = 1000
): number {
  const linearDelay = attempt * baseDelayMs;
  const jitter = Math.random() * baseDelayMs;
  return linearDelay + jitter;
}

/**
 * Immediate retry with no delay (risky, use with caution)
 */
export function immediateRetry(attempt: number): number {
  return Math.random() * 100; // Minimal jitter only
}

/**
 * Create a retry policy with sensible defaults
 */
export function createDefaultRetryPolicy(): RetryPolicyConfig {
  return {
    maxRetries: 3,
    initialDelayMs: 1000,
    maxDelayMs: 32000, // 32 seconds
    backoffStrategy: exponentialBackoff,
    retryableStatusCodes: [429, 500, 502, 503, 504],
    retryableErrors: [
      TypeError, // Network errors often throw TypeError
      RangeError,
    ],
  };
}

/**
 * Create a retry policy for aggressive retry scenarios (many retries)
 */
export function createAggressiveRetryPolicy(): RetryPolicyConfig {
  return {
    maxRetries: 5,
    initialDelayMs: 500,
    maxDelayMs: 60000, // 60 seconds
    backoffStrategy: exponentialBackoff,
    retryableStatusCodes: [429, 500, 502, 503, 504],
    retryableErrors: [TypeError, RangeError],
  };
}

/**
 * Create a retry policy for conservative retry scenarios (minimal retries)
 */
export function createConservativeRetryPolicy(): RetryPolicyConfig {
  return {
    maxRetries: 1,
    initialDelayMs: 2000,
    maxDelayMs: 5000,
    backoffStrategy: linearBackoff,
    retryableStatusCodes: [503], // Only retry on service unavailable
    retryableErrors: [TypeError],
  };
}

/**
 * Retry policy manager
 */
export class RetryPolicy {
  private config: RetryPolicyConfig;
  private metrics: Map<string, RetryMetrics> = new Map();

  constructor(config?: Partial<RetryPolicyConfig>) {
    this.config = {
      ...createDefaultRetryPolicy(),
      ...config,
    };
  }

  /**
   * Check if an error should be retried
   */
  public shouldRetry(
    error: Error | Response,
    attempt: number
  ): boolean {
    if (attempt >= this.config.maxRetries) {
      return false;
    }

    // Check custom retry decision first
    if (this.config.customRetryDecision) {
      return this.config.customRetryDecision(
        error,
        attempt,
        this.getMetrics()
      );
    }

    // Check HTTP status codes
    if (error instanceof Response) {
      return this.config.retryableStatusCodes.includes(error.status);
    }

    // Check error types
    for (const ErrorType of this.config.retryableErrors) {
      if (error instanceof ErrorType) {
        return true;
      }
    }

    return false;
  }

  /**
   * Calculate delay for retry attempt
   */
  public getDelayMs(attempt: number, retryAfterHeader?: string): number {
    // Check for Retry-After header first
    if (retryAfterHeader) {
      const parsed = parseFloat(retryAfterHeader);
      if (!isNaN(parsed)) {
        return Math.min(parsed * 1000, this.config.maxDelayMs);
      }
    }

    // Use backoff strategy
    const delay = this.config.backoffStrategy(
      attempt,
      this.config.initialDelayMs
    );
    return Math.min(delay, this.config.maxDelayMs);
  }

  /**
   * Record retry attempt
   */
  public recordRetry(
    requestKey: string,
    attempt: number,
    delay: number,
    reason: string
  ): void {
    let metrics = this.metrics.get(requestKey);
    if (!metrics) {
      metrics = {
        totalAttempts: 1,
        totalRetries: 0,
        totalDelayMs: 0,
      };
      this.metrics.set(requestKey, metrics);
    }

    metrics.totalRetries++;
    metrics.totalDelayMs += delay;
    metrics.lastRetryTime = new Date();

    if (this.config.onRetry) {
      this.config.onRetry(attempt, delay, reason);
    }
  }

  /**
   * Record successful response
   */
  public recordSuccess(requestKey: string, attempt: number): void {
    const metrics = this.metrics.get(requestKey);
    if (metrics) {
      metrics.successOnAttempt = attempt;
    }
  }

  /**
   * Get metrics for a request or all requests
   */
  public getMetrics(requestKey?: string): RetryMetrics {
    if (requestKey) {
      return this.metrics.get(requestKey) || {
        totalAttempts: 0,
        totalRetries: 0,
        totalDelayMs: 0,
      };
    }

    // Aggregate all metrics
    let totalAttempts = 0;
    let totalRetries = 0;
    let totalDelayMs = 0;

    for (const metrics of this.metrics.values()) {
      totalAttempts += metrics.totalAttempts;
      totalRetries += metrics.totalRetries;
      totalDelayMs += metrics.totalDelayMs;
    }

    return {
      totalAttempts,
      totalRetries,
      totalDelayMs,
    };
  }

  /**
   * Clear metrics
   */
  public clearMetrics(): void {
    this.metrics.clear();
  }

  /**
   * Update configuration
   */
  public updateConfig(config: Partial<RetryPolicyConfig>): void {
    this.config = {
      ...this.config,
      ...config,
    };
  }

  /**
   * Get current configuration
   */
  public getConfig(): RetryPolicyConfig {
    return { ...this.config };
  }
}

/**
 * Singleton instance for global retry policy
 */
let globalRetryPolicy: RetryPolicy | null = null;

/**
 * Get or create global retry policy
 */
export function getGlobalRetryPolicy(
  config?: Partial<RetryPolicyConfig>
): RetryPolicy {
  if (!globalRetryPolicy) {
    globalRetryPolicy = new RetryPolicy(config);
  }
  return globalRetryPolicy;
}

/**
 * Reset global retry policy
 */
export function resetGlobalRetryPolicy(): void {
  globalRetryPolicy = null;
}
