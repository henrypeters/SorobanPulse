package soroban_pulse

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

// Client represents a Soroban Pulse API client
type Client struct {
	baseURL    string
	apiKey     string
	httpClient *http.Client
	retryPolicy *RetryPolicy
}

// ClientConfig holds configuration for the client
type ClientConfig struct {
	BaseURL          string
	APIKey           string
	Timeout          time.Duration
	MaxRetries       int
	RetryInitialDelay time.Duration
	RetryMaxDelay    time.Duration
	RetryableStatusCodes []int
	OnRetry          func(attempt int, delay time.Duration, reason string)
}

// NewClient creates a new Soroban Pulse API client
func NewClient(config ClientConfig) *Client {
	if config.BaseURL == "" {
		config.BaseURL = "https://api.sorobanpulse.com"
	}
	if config.Timeout == 0 {
		config.Timeout = 30 * time.Second
	}
	if config.MaxRetries == 0 {
		config.MaxRetries = 3
	}
	if config.RetryInitialDelay == 0 {
		config.RetryInitialDelay = 1 * time.Second
	}
	if config.RetryMaxDelay == 0 {
		config.RetryMaxDelay = 32 * time.Second
	}
	if len(config.RetryableStatusCodes) == 0 {
		config.RetryableStatusCodes = []int{429, 500, 502, 503, 504}
	}

	httpClient := &http.Client{
		Timeout: config.Timeout,
	}

	retryPolicy := &RetryPolicy{
		MaxRetries:              config.MaxRetries,
		InitialDelay:            config.RetryInitialDelay,
		MaxDelay:                config.RetryMaxDelay,
		RetryableStatusCodes:    config.RetryableStatusCodes,
		OnRetry:                 config.OnRetry,
	}

	return &Client{
		baseURL:     strings.TrimSuffix(config.BaseURL, "/"),
		apiKey:      config.APIKey,
		httpClient:  httpClient,
		retryPolicy: retryPolicy,
	}
}

// GetEvents retrieves events with optional filtering
func (c *Client) GetEvents(ctx context.Context, opts *GetEventsOptions) (*EventsResponse, error) {
	if opts == nil {
		opts = &GetEventsOptions{}
	}

	params := url.Values{}
	params.Add("page", fmt.Sprintf("%d", opts.Page))
	params.Add("limit", fmt.Sprintf("%d", opts.Limit))

	if opts.ExactCount {
		params.Add("exact_count", "true")
	}
	if opts.EventType != "" {
		params.Add("event_type", opts.EventType)
	}
	if opts.FromLedger > 0 {
		params.Add("from_ledger", fmt.Sprintf("%d", opts.FromLedger))
	}
	if opts.ToLedger > 0 {
		params.Add("to_ledger", fmt.Sprintf("%d", opts.ToLedger))
	}

	url := fmt.Sprintf("%s/v1/events?%s", c.baseURL, params.Encode())

	resp, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}

	var result EventsResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}

	return &result, nil
}

// GetEventsByContract retrieves events for a specific contract
func (c *Client) GetEventsByContract(ctx context.Context, contractID string, opts *GetEventsOptions) (*EventsResponse, error) {
	if contractID == "" {
		return nil, fmt.Errorf("contract ID is required")
	}

	params := url.Values{}
	params.Add("page", fmt.Sprintf("%d", opts.Page))
	params.Add("limit", fmt.Sprintf("%d", opts.Limit))

	url := fmt.Sprintf("%s/v1/events/%s?%s", c.baseURL, contractID, params.Encode())

	resp, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}

	var result EventsResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}

	return &result, nil
}

// GetEventsByTransactionHash retrieves events for a specific transaction
func (c *Client) GetEventsByTransactionHash(ctx context.Context, txHash string) (*EventsResponse, error) {
	if txHash == "" {
		return nil, fmt.Errorf("transaction hash is required")
	}

	url := fmt.Sprintf("%s/v1/events/tx/%s", c.baseURL, txHash)

	resp, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}

	var result EventsResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}

	return &result, nil
}

// StreamEvents streams events using SSE
// The provided handler function will be called for each event
func (c *Client) StreamEvents(ctx context.Context, contractID *string, handler func(*Event) error) error {
	var streamURL string
	if contractID != nil {
		streamURL = fmt.Sprintf("%s/v1/events/stream?contract_id=%s", c.baseURL, *contractID)
	} else {
		streamURL = fmt.Sprintf("%s/v1/events/stream", c.baseURL)
	}

	req, err := http.NewRequestWithContext(ctx, "GET", streamURL, nil)
	if err != nil {
		return err
	}

	c.setHeaders(req)
	req.Header.Set("Accept", "text/event-stream")

	resp, err := c.doRequestWithRetry(ctx, req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("unexpected status code: %d, body: %s", resp.StatusCode, string(body))
	}

	reader := io.NewReader(resp.Body)
	buffer := make([]byte, 0, 64*1024)
	for {
		line, err := reader.ReadBytes('\n')
		if err != nil && err != io.EOF {
			return err
		}

		if len(line) > 0 {
			line = bytes.TrimSuffix(line, []byte("\n"))
			if bytes.HasPrefix(line, []byte("data: ")) {
				data := line[6:]
				if len(data) > 0 {
					var event Event
					if err := json.Unmarshal(data, &event); err != nil {
						// Log but continue on parse errors
						continue
					}
					if err := handler(&event); err != nil {
						return err
					}
				}
			}
		}

		if err == io.EOF {
			break
		}
	}

	return nil
}

// GetHealth checks the service health
func (c *Client) GetHealth(ctx context.Context) (*HealthResponse, error) {
	url := fmt.Sprintf("%s/healthz/ready", c.baseURL)

	resp, err := c.doRequest(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}

	var result HealthResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}

	return &result, nil
}

// doRequest performs an HTTP request
func (c *Client) doRequest(ctx context.Context, method, url string, body io.Reader) (*http.Response, error) {
	req, err := http.NewRequestWithContext(ctx, method, url, body)
	if err != nil {
		return nil, err
	}

	c.setHeaders(req)
	return c.doRequestWithRetry(ctx, req)
}

// doRequestWithRetry performs an HTTP request with retry logic
func (c *Client) doRequestWithRetry(ctx context.Context, req *http.Request) (*http.Response, error) {
	var lastResp *http.Response
	var lastErr error

	for attempt := 0; attempt <= c.retryPolicy.MaxRetries; attempt++ {
		if attempt > 0 {
			// Calculate backoff delay
			delay := c.retryPolicy.CalculateDelay(attempt, lastResp)

			if c.retryPolicy.OnRetry != nil {
				reason := "connection error"
				if lastResp != nil {
					reason = fmt.Sprintf("HTTP %d", lastResp.StatusCode)
				}
				c.retryPolicy.OnRetry(attempt, delay, reason)
			}

			// Apply jitter and wait
			select {
			case <-time.After(delay):
			case <-ctx.Done():
				return nil, ctx.Err()
			}

			// Create a new request for retry
			newReq, err := http.NewRequestWithContext(ctx, req.Method, req.URL.String(), nil)
			if err != nil {
				return nil, err
			}
			newReq.Header = req.Header.Clone()
			req = newReq
		}

		resp, err := c.httpClient.Do(req)
		if err != nil {
			lastErr = err
			if attempt < c.retryPolicy.MaxRetries {
				continue
			}
			return nil, err
		}

		lastResp = resp

		// Check if we should retry based on status code
		if resp.StatusCode >= 200 && resp.StatusCode < 300 {
			// Success
			return resp, nil
		}

		if !c.retryPolicy.ShouldRetry(resp.StatusCode) || attempt >= c.retryPolicy.MaxRetries {
			// Don't retry this status code or out of retries
			return resp, nil
		}

		// Close the response body before retrying
		resp.Body.Close()
	}

	// This shouldn't be reached
	if lastResp != nil {
		return lastResp, nil
	}
	return nil, lastErr
}

// setHeaders sets common headers for requests
func (c *Client) setHeaders(req *http.Request) {
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", "soroban-pulse-go/1.0")

	if c.apiKey != "" {
		req.Header.Set("X-Api-Key", c.apiKey)
	}
}

// Close closes any resources held by the client
func (c *Client) Close() error {
	// Currently no resources to close, but this is here for future use
	return nil
}
