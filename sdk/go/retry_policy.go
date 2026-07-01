package soroban_pulse

import (
	"math"
	"math/rand"
	"net/http"
	"time"
)

// RetryPolicy defines retry behavior
type RetryPolicy struct {
	MaxRetries              int
	InitialDelay            time.Duration
	MaxDelay                time.Duration
	RetryableStatusCodes    []int
	OnRetry                 func(attempt int, delay time.Duration, reason string)
}

// ShouldRetry determines if a response should be retried
func (rp *RetryPolicy) ShouldRetry(statusCode int) bool {
	for _, code := range rp.RetryableStatusCodes {
		if code == statusCode {
			return true
		}
	}
	return false
}

// CalculateDelay calculates the delay before the next retry attempt
// Implements exponential backoff with jitter
func (rp *RetryPolicy) CalculateDelay(attempt int, resp *http.Response) time.Duration {
	// Check for Retry-After header first
	if resp != nil {
		if retryAfter := resp.Header.Get("Retry-After"); retryAfter != "" {
			// Try to parse as seconds
			if seconds, err := time.ParseDuration(retryAfter + "s"); err == nil {
				if seconds > rp.MaxDelay {
					return rp.MaxDelay
				}
				return seconds
			}
		}
	}

	// Calculate exponential backoff with jitter
	// delay = 2^attempt * initialDelay + random(0, initialDelay)
	exponentialComponent := math.Pow(2, float64(attempt))
	baseDelay := time.Duration(exponentialComponent) * rp.InitialDelay
	jitter := time.Duration(rand.Int63n(int64(rp.InitialDelay)))
	delay := baseDelay + jitter

	// Cap at max delay
	if delay > rp.MaxDelay {
		return rp.MaxDelay
	}

	return delay
}

// DefaultRetryPolicy returns a default retry policy
func DefaultRetryPolicy() *RetryPolicy {
	return &RetryPolicy{
		MaxRetries:           3,
		InitialDelay:         1 * time.Second,
		MaxDelay:             32 * time.Second,
		RetryableStatusCodes: []int{429, 500, 502, 503, 504},
	}
}

// AggressiveRetryPolicy returns a retry policy for critical operations
func AggressiveRetryPolicy() *RetryPolicy {
	return &RetryPolicy{
		MaxRetries:           5,
		InitialDelay:         500 * time.Millisecond,
		MaxDelay:             60 * time.Second,
		RetryableStatusCodes: []int{429, 500, 502, 503, 504},
	}
}

// ConservativeRetryPolicy returns a retry policy that fails fast
func ConservativeRetryPolicy() *RetryPolicy {
	return &RetryPolicy{
		MaxRetries:           1,
		InitialDelay:         2 * time.Second,
		MaxDelay:             5 * time.Second,
		RetryableStatusCodes: []int{503}, // Only retry service unavailable
	}
}
