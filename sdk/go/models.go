package soroban_pulse

import "time"

// Event represents a Soroban contract event
type Event struct {
	ID              string                 `json:"id"`
	ContractID      string                 `json:"contract_id"`
	EventType       string                 `json:"event_type"`
	TxHash          string                 `json:"tx_hash"`
	Ledger          int64                  `json:"ledger"`
	Timestamp       time.Time              `json:"timestamp"`
	EventData       map[string]interface{} `json:"event_data"`
	CreatedAt       time.Time              `json:"created_at"`
}

// EventsResponse represents the API response for event list queries
type EventsResponse struct {
	Data        []Event `json:"data"`
	Total       int64   `json:"total"`
	Page        int     `json:"page"`
	Limit       int     `json:"limit"`
	Approximate bool    `json:"approximate"`
}

// HealthResponse represents the service health status
type HealthResponse struct {
	Status   string `json:"status"`
	Database string `json:"db"`
	Indexer  string `json:"indexer"`
}

// ContractSummary represents a summary of contract activity
type ContractSummary struct {
	ContractID    string    `json:"contract_id"`
	EventCount    int64     `json:"event_count"`
	FirstEventAt  time.Time `json:"first_event_at"`
	LastEventAt   time.Time `json:"last_event_at"`
	LastUpdatedAt time.Time `json:"last_updated_at"`
}

// GetEventsOptions holds options for getting events
type GetEventsOptions struct {
	Page      int
	Limit     int
	ExactCount bool
	EventType  string // "contract", "diagnostic", "system"
	FromLedger int64
	ToLedger   int64
}

// NewGetEventsOptions creates a new GetEventsOptions with defaults
func NewGetEventsOptions() *GetEventsOptions {
	return &GetEventsOptions{
		Page:   1,
		Limit:  20,
	}
}

// PaginationParams represents pagination parameters
type PaginationParams struct {
	Page      int  `json:"page"`
	Limit     int  `json:"limit"`
	ExactCount bool `json:"exact_count"`
}

// EventType represents the type of event
type EventType string

const (
	EventTypeContract    EventType = "contract"
	EventTypeDiagnostic  EventType = "diagnostic"
	EventTypeSystem      EventType = "system"
)
