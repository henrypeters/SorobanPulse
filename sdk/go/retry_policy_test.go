package soroban_pulse

import (
	"net/http"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestRetryPolicyShouldRetry(t *testing.T) {
	tests := []struct {
		name              string
		statusCode        int
		retryableCodes    []int
		shouldRetry       bool
	}{
		{
			name:           "Should retry 429",
			statusCode:     429,
			retryableCodes: []int{429, 500, 502, 503, 504},
			shouldRetry:    true,
		},
		{
			name:           "Should retry 503",
			statusCode:     503,
			retryableCodes: []int{429, 500, 502, 503, 504},
			shouldRetry:    true,
		},
		{
			name:           "Should not retry 200",
			statusCode:     200,
			retryableCodes: []int{429, 500, 502, 503, 504},
			shouldRetry:    false,
		},
		{
			name:           "Should not retry 404",
			statusCode:     404,
			retryableCodes: []int{429, 500, 502, 503, 504},
			shouldRetry:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			policy := &RetryPolicy{
				RetryableStatusCodes: tt.retryableCodes,
			}
			assert.Equal(t, tt.shouldRetry, policy.ShouldRetry(tt.statusCode))
		})
	}
}

func TestCalculateDelayExponentialBackoff(t *testing.T) {
	policy := &RetryPolicy{
		InitialDelay: 1 * time.Second,
		MaxDelay:     32 * time.Second,
	}

	tests := []struct {
		name              string
		attempt           int
		minExpectedDelay  time.Duration
		maxExpectedDelay  time.Duration
	}{
		{
			name:             "Attempt 0",
			attempt:          0,
			minExpectedDelay: 1 * time.Second,
			maxExpectedDelay: 2 * time.Second,
		},
		{
			name:             "Attempt 1",
			attempt:          1,
			minExpectedDelay: 2 * time.Second,
			maxExpectedDelay: 4 * time.Second,
		},
		{
			name:             "Attempt 2",
			attempt:          2,
			minExpectedDelay: 4 * time.Second,
			maxExpectedDelay: 8 * time.Second,
		},
		{
			name:             "Attempt 3",
			attempt:          3,
			minExpectedDelay: 8 * time.Second,
			maxExpectedDelay: 16 * time.Second,
		},
		{
			name:             "Attempt 4",
			attempt:          4,
			minExpectedDelay: 16 * time.Second,
			maxExpectedDelay: 32 * time.Second,
		},
		{
			name:             "Attempt 5 (capped)",
			attempt:          5,
			minExpectedDelay: 32 * time.Second,
			maxExpectedDelay: 32 * time.Second,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			delay := policy.CalculateDelay(tt.attempt, nil)
			assert.GreaterOrEqual(t, delay, tt.minExpectedDelay)
			assert.LessOrEqual(t, delay, tt.maxExpectedDelay)
		})
	}
}

func TestCalculateDelayWithRetryAfter(t *testing.T) {
	policy := &RetryPolicy{
		InitialDelay: 1 * time.Second,
		MaxDelay:     32 * time.Second,
	}

	resp := &http.Response{
		Header: make(http.Header),
	}
	resp.Header.Set("Retry-After", "10")

	delay := policy.CalculateDelay(0, resp)
	assert.Equal(t, 10*time.Second, delay)
}

func TestCalculateDelayMaxDelayCap(t *testing.T) {
	policy := &RetryPolicy{
		InitialDelay: 1 * time.Second,
		MaxDelay:     5 * time.Second,
	}

	resp := &http.Response{
		Header: make(http.Header),
	}
	resp.Header.Set("Retry-After", "100")

	delay := policy.CalculateDelay(0, resp)
	assert.Equal(t, 5*time.Second, delay)
}

func TestDefaultRetryPolicy(t *testing.T) {
	policy := DefaultRetryPolicy()
	assert.Equal(t, 3, policy.MaxRetries)
	assert.Equal(t, 1*time.Second, policy.InitialDelay)
	assert.Equal(t, 32*time.Second, policy.MaxDelay)
	assert.Equal(t, []int{429, 500, 502, 503, 504}, policy.RetryableStatusCodes)
}

func TestAggressiveRetryPolicy(t *testing.T) {
	policy := AggressiveRetryPolicy()
	assert.Equal(t, 5, policy.MaxRetries)
	assert.Equal(t, 500*time.Millisecond, policy.InitialDelay)
	assert.Equal(t, 60*time.Second, policy.MaxDelay)
}

func TestConservativeRetryPolicy(t *testing.T) {
	policy := ConservativeRetryPolicy()
	assert.Equal(t, 1, policy.MaxRetries)
	assert.Equal(t, 2*time.Second, policy.InitialDelay)
	assert.Equal(t, 5*time.Second, policy.MaxDelay)
	assert.Equal(t, []int{503}, policy.RetryableStatusCodes)
}

func TestRetryCallback(t *testing.T) {
	callCount := 0
	policy := &RetryPolicy{
		MaxRetries:           3,
		InitialDelay:         1 * time.Second,
		MaxDelay:             32 * time.Second,
		RetryableStatusCodes: []int{429, 500, 502, 503, 504},
		OnRetry: func(attempt int, delay time.Duration, reason string) {
			callCount++
			assert.Equal(t, "HTTP 429", reason)
		},
	}

	resp := &http.Response{
		StatusCode: 429,
		Header:     make(http.Header),
	}

	// First retry
	policy.OnRetry(1, 1*time.Second, "HTTP 429")
	assert.Equal(t, 1, callCount)

	// Second retry
	policy.OnRetry(2, 2*time.Second, "HTTP 429")
	assert.Equal(t, 2, callCount)
}
