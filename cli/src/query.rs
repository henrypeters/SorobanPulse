use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// API response shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SorobanEvent {
    pub contract_id:       String,
    pub event_type:        String,
    pub tx_hash:           String,
    pub ledger:            i64,
    pub ledger_closed_at:  String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_hash:       Option<String>,
    pub in_successful_call: bool,
    pub value:             Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic:             Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Contract {
    pub contract_id:   String,
    pub event_count:   Option<i64>,
    pub first_seen:    Option<String>,
    pub last_seen:     Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EventStats {
    pub total_events:     i64,
    pub total_contracts:  i64,
    pub latest_ledger:    Option<i64>,
    pub events_last_24h:  Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct PaginatedEvents {
    pub events:  Vec<SorobanEvent>,
    pub page:    Option<u32>,
    pub limit:   Option<u32>,
    pub total:   Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EventsResponse {
    Paginated(PaginatedEvents),
    List(Vec<SorobanEvent>),
}

impl EventsResponse {
    pub fn into_events(self) -> Vec<SorobanEvent> {
        match self {
            Self::Paginated(p) => p.events,
            Self::List(v)      => v,
        }
    }
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct EventQuery {
    pub from_ledger:  Option<i64>,
    pub to_ledger:    Option<i64>,
    pub contract_id:  Option<String>,
    pub event_type:   Option<String>,
    pub tx_hash:      Option<String>,
    pub limit:        u32,
    pub page:         u32,
    pub sort:         String,
    pub sort_by:      String,
}

impl EventQuery {
    pub fn to_params(&self) -> Vec<(String, String)> {
        let mut p = vec![
            ("limit".into(),   self.limit.to_string()),
            ("page".into(),    self.page.to_string()),
            ("sort".into(),    self.sort.clone()),
            ("sort_by".into(), self.sort_by.clone()),
        ];
        if let Some(v) = self.from_ledger  { p.push(("from_ledger".into(), v.to_string())); }
        if let Some(v) = self.to_ledger    { p.push(("to_ledger".into(),   v.to_string())); }
        if let Some(v) = &self.contract_id { p.push(("contract_id".into(), v.clone())); }
        if let Some(v) = &self.event_type  { p.push(("event_type".into(),  v.clone())); }
        if let Some(v) = &self.tx_hash     { p.push(("tx_hash".into(),     v.clone())); }
        p
    }

    /// Build the API path — use the contract-specific endpoint when a
    /// contract_id filter is set, otherwise the general /v1/events path.
    pub fn path(&self) -> String {
        match &self.contract_id {
            Some(id) => format!("/v1/events/contract/{id}"),
            None     => "/v1/events".into(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ContractQuery {
    pub search: Option<String>,
    pub limit:  u32,
    pub page:   u32,
}

impl ContractQuery {
    pub fn path(&self) -> &'static str {
        "/v1/contracts"
    }

    pub fn to_params(&self) -> Vec<(String, String)> {
        let mut p = vec![
            ("limit".into(), self.limit.to_string()),
            ("page".into(),  self.page.to_string()),
        ];
        if let Some(s) = &self.search { p.push(("q".into(), s.clone())); }
        p
    }
}
