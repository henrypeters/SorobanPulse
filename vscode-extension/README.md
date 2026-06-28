# Soroban Pulse Explorer

Browse, test, and inspect [Soroban Pulse](https://github.com/soroban-pulse/soroban-pulse) API endpoints directly from VS Code.

## Features

- **API Explorer** — sidebar tree of all endpoints grouped by category (Events, Contracts, Subscriptions, Admin, …)
- **Request Tester** — send real HTTP requests with path params, query params, custom headers, and a body editor
- **Response Viewer** — formatted JSON body, status badge, duration, and response headers

## Getting Started

1. Install the extension.
2. Open **Settings** (`Ctrl+,`) and search for `sorobanpulse`:
   - Set `sorobanpulse.baseUrl` to your running instance (default: `http://localhost:3000`)
   - Set `sorobanpulse.apiKey` for authenticated endpoints
   - Optionally set `sorobanpulse.adminApiKey` for `/admin/*` endpoints
3. Click the **⚡** icon in the activity bar to open the API Explorer.
4. Click any endpoint to open it in the Request Tester — fill in parameters and hit **Send**.

## Commands

| Command | Description |
|---------|-------------|
| `Soroban Pulse: Open Settings` | Jump to extension settings |
| Refresh (toolbar) | Reload the endpoint list |
| Copy URL (right-click) | Copy the full endpoint URL to clipboard |

## Publishing

```bash
cd vscode-extension
npm install
npm run package        # builds soroban-pulse-explorer-x.x.x.vsix
npm run publish        # publishes to VS Code Marketplace (requires vsce login)
```

## Requirements

- VS Code `^1.85.0`
- A running Soroban Pulse server
