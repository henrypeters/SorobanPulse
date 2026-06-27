import { ApiEndpoint, EndpointGroup } from './types';

export const API_GROUPS: EndpointGroup[] = [
    {
        label: 'Health & System',
        endpoints: [
            {
                method: 'GET', path: '/health',
                description: 'Basic health check', auth: 'none',
            },
            {
                method: 'GET', path: '/healthz/live',
                description: 'Liveness probe', auth: 'none',
            },
            {
                method: 'GET', path: '/healthz/ready',
                description: 'Readiness probe', auth: 'none',
            },
            {
                method: 'GET', path: '/status',
                description: 'Indexer status and lag metrics', auth: 'api-key',
            },
            {
                method: 'GET', path: '/metrics',
                description: 'Prometheus metrics scrape endpoint', auth: 'none',
            },
        ],
    },
    {
        label: 'Events',
        endpoints: [
            {
                method: 'GET', path: '/v1/events',
                description: 'List Soroban events with filtering and pagination',
                auth: 'api-key',
                params: [
                    { name: 'page', in: 'query', required: false, example: '1' },
                    { name: 'limit', in: 'query', required: false, example: '25' },
                    { name: 'event_type', in: 'query', required: false, example: 'contract' },
                    { name: 'from_ledger', in: 'query', required: false, example: '1000000' },
                    { name: 'to_ledger', in: 'query', required: false },
                    { name: 'sort', in: 'query', required: false, example: 'desc' },
                    { name: 'sort_by', in: 'query', required: false, example: 'ledger' },
                    { name: 'exact_count', in: 'query', required: false, example: 'false' },
                ],
            },
            {
                method: 'GET', path: '/v1/events/recent',
                description: 'Most recently indexed events', auth: 'api-key',
            },
            {
                method: 'GET', path: '/v1/events/stats',
                description: 'Aggregate event statistics', auth: 'api-key',
            },
            {
                method: 'GET', path: '/v1/events/diff',
                description: 'Events changed between two ledger ranges', auth: 'api-key',
                params: [
                    { name: 'from_ledger', in: 'query', required: true },
                    { name: 'to_ledger', in: 'query', required: true },
                ],
            },
            {
                method: 'GET', path: '/v1/events/timeseries',
                description: 'Event volume over time buckets', auth: 'api-key',
                params: [
                    { name: 'bucket', in: 'query', required: false, example: 'hour' },
                    { name: 'from', in: 'query', required: false },
                    { name: 'to', in: 'query', required: false },
                ],
            },
            {
                method: 'GET', path: '/v1/events/export',
                description: 'Export events as CSV or Parquet',
                auth: 'api-key',
                params: [
                    { name: 'format', in: 'query', required: false, example: 'csv' },
                    { name: 'contract_id', in: 'query', required: false },
                    { name: 'from_ledger', in: 'query', required: false },
                    { name: 'to_ledger', in: 'query', required: false },
                ],
            },
            {
                method: 'GET', path: '/v1/events/stream',
                description: 'SSE stream of live events', auth: 'api-key',
                streaming: true,
                params: [
                    { name: 'contract_id', in: 'query', required: false },
                ],
            },
            {
                method: 'GET', path: '/v1/events/stream/multi',
                description: 'SSE stream for multiple contracts', auth: 'api-key',
                streaming: true,
            },
            {
                method: 'POST', path: '/v1/events/tx/batch',
                description: 'Fetch events for multiple transaction hashes',
                auth: 'api-key',
                bodyExample: JSON.stringify({ tx_hashes: ['abc123', 'def456'] }, null, 2),
            },
        ],
    },
    {
        label: 'Contracts',
        endpoints: [
            {
                method: 'GET', path: '/v1/events/contract/{contract_id}',
                description: 'Events for a specific contract',
                auth: 'api-key',
                params: [
                    { name: 'contract_id', in: 'path', required: true, example: 'CABC...' },
                    { name: 'page', in: 'query', required: false },
                    { name: 'limit', in: 'query', required: false, example: '25' },
                    { name: 'cursor', in: 'query', required: false },
                    { name: 'from_ledger', in: 'query', required: false },
                    { name: 'to_ledger', in: 'query', required: false },
                ],
            },
            {
                method: 'GET', path: '/v1/events/contract/{contract_id}/stream',
                description: 'SSE stream for a specific contract',
                auth: 'api-key', streaming: true,
                params: [
                    { name: 'contract_id', in: 'path', required: true },
                ],
            },
            {
                method: 'GET', path: '/v1/contracts',
                description: 'List all indexed contracts', auth: 'api-key',
                params: [
                    { name: 'page', in: 'query', required: false },
                    { name: 'limit', in: 'query', required: false },
                ],
            },
            {
                method: 'GET', path: '/v1/contracts/search',
                description: 'Search contracts by partial ID', auth: 'api-key',
                params: [
                    { name: 'q', in: 'query', required: true },
                ],
            },
            {
                method: 'GET', path: '/v1/contracts/{contract_id}/summary',
                description: 'Event summary for a contract',
                auth: 'api-key',
                params: [
                    { name: 'contract_id', in: 'path', required: true },
                ],
            },
            {
                method: 'GET', path: '/v1/contracts/{contract_id}/stats/history',
                description: 'Historical stats for a contract',
                auth: 'api-key',
                params: [
                    { name: 'contract_id', in: 'path', required: true },
                    { name: 'bucket', in: 'query', required: false, example: 'day' },
                    { name: 'days', in: 'query', required: false, example: '30' },
                ],
            },
        ],
    },
    {
        label: 'Transactions',
        endpoints: [
            {
                method: 'GET', path: '/v1/events/tx/{tx_hash}',
                description: 'Events for a transaction hash',
                auth: 'api-key',
                params: [
                    { name: 'tx_hash', in: 'path', required: true },
                ],
            },
            {
                method: 'GET', path: '/v1/events/tx/{tx_hash}/related',
                description: 'Related events linked to a transaction',
                auth: 'api-key',
                params: [
                    { name: 'tx_hash', in: 'path', required: true },
                    { name: 'depth', in: 'query', required: false, example: '1' },
                ],
            },
            {
                method: 'GET', path: '/v1/events/ledger-hash/{hash}',
                description: 'Events for a ledger hash',
                auth: 'api-key',
                params: [
                    { name: 'hash', in: 'path', required: true },
                ],
            },
        ],
    },
    {
        label: 'Subscriptions',
        endpoints: [
            {
                method: 'POST', path: '/subscriptions',
                description: 'Create a webhook subscription',
                auth: 'api-key',
                bodyExample: JSON.stringify({ callback_url: 'https://example.com/hook', from_ledger: 1000000 }, null, 2),
            },
            {
                method: 'GET', path: '/subscriptions/{id}',
                description: 'Get subscription details and pending count',
                auth: 'api-key',
                params: [{ name: 'id', in: 'path', required: true, example: 'uuid' }],
            },
            {
                method: 'DELETE', path: '/subscriptions/{id}',
                description: 'Cancel an active subscription',
                auth: 'api-key',
                params: [{ name: 'id', in: 'path', required: true }],
            },
            {
                method: 'POST', path: '/subscriptions/{id}/ack',
                description: 'Advance the acknowledged ledger cursor',
                auth: 'api-key',
                params: [{ name: 'id', in: 'path', required: true }],
                bodyExample: JSON.stringify({ ledger: 1000100 }, null, 2),
            },
        ],
    },
    {
        label: 'Admin',
        endpoints: [
            {
                method: 'POST', path: '/admin/replay',
                description: 'Replay events from a ledger range',
                auth: 'admin-key',
                bodyExample: JSON.stringify({ from_ledger: 1000000, to_ledger: 1001000 }, null, 2),
            },
            {
                method: 'POST', path: '/admin/indexer/pause',
                description: 'Pause the event indexer', auth: 'admin-key',
            },
            {
                method: 'POST', path: '/admin/indexer/resume',
                description: 'Resume the event indexer', auth: 'admin-key',
            },
            {
                method: 'GET', path: '/admin/contracts/{contract_id}/abi',
                description: 'Get registered ABI for a contract',
                auth: 'admin-key',
                params: [{ name: 'contract_id', in: 'path', required: true }],
            },
            {
                method: 'POST', path: '/admin/contracts/{contract_id}/abi',
                description: 'Register a contract ABI',
                auth: 'admin-key',
                params: [{ name: 'contract_id', in: 'path', required: true }],
                bodyExample: JSON.stringify([{ name: 'transfer', inputs: [] }], null, 2),
            },
            {
                method: 'POST', path: '/admin/contracts/{contract_id}/schema',
                description: 'Register a JSON schema for event validation',
                auth: 'admin-key',
                params: [{ name: 'contract_id', in: 'path', required: true }],
                bodyExample: JSON.stringify({ type: 'object', properties: {} }, null, 2),
            },
            {
                method: 'DELETE', path: '/admin/contracts/{contract_id}/schema',
                description: 'Delete a contract schema',
                auth: 'admin-key',
                params: [{ name: 'contract_id', in: 'path', required: true }],
            },
            {
                method: 'POST', path: '/admin/notifications/channels',
                description: 'Create a notification channel',
                auth: 'admin-key',
                bodyExample: JSON.stringify({
                    name: 'my-webhook',
                    channel_type: 'webhook',
                    config: { url: 'https://example.com/hook', secret: 'mysecret' },
                    retry_policy: { max_attempts: 5, base_delay_secs: 2 },
                }, null, 2),
            },
            {
                method: 'POST', path: '/admin/notifications/suppress',
                description: 'Add a URL/address to the suppression list',
                auth: 'admin-key',
                bodyExample: JSON.stringify({ target: 'https://example.com/hook', target_type: 'webhook' }, null, 2),
            },
            {
                method: 'POST', path: '/admin/events/{id}/anonymize',
                description: 'Anonymize an event by ID',
                auth: 'admin-key',
                params: [{ name: 'id', in: 'path', required: true }],
            },
            {
                method: 'POST', path: '/admin/bulk-insert',
                description: 'Bulk insert events',
                auth: 'admin-key',
                bodyExample: JSON.stringify({ events: [] }, null, 2),
            },
        ],
    },
    {
        label: 'Docs',
        endpoints: [
            {
                method: 'GET', path: '/openapi.json',
                description: 'OpenAPI 3.0 specification', auth: 'none',
            },
            {
                method: 'GET', path: '/docs',
                description: 'Swagger UI', auth: 'none',
            },
        ],
    },
];
