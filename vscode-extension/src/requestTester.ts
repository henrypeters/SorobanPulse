import * as vscode from 'vscode';
import * as https from 'https';
import * as http from 'http';
import { URL } from 'url';
import { ApiEndpoint, RequestConfig, ResponseData, WebviewMessage } from './types';

// ---------------------------------------------------------------------------
// Panel manager — single reusable panel instance
// ---------------------------------------------------------------------------

export class RequestTesterPanel {
    static currentPanel: RequestTesterPanel | undefined;
    private static readonly viewType = 'sorobanpulse.requestTester';

    private readonly _panel: vscode.WebviewPanel;
    private readonly _extensionUri: vscode.Uri;
    private _disposables: vscode.Disposable[] = [];

    static open(extensionUri: vscode.Uri, endpoint?: ApiEndpoint): void {
        const column = vscode.window.activeTextEditor?.viewColumn ?? vscode.ViewColumn.One;

        if (RequestTesterPanel.currentPanel) {
            RequestTesterPanel.currentPanel._panel.reveal(column);
            if (endpoint) {
                RequestTesterPanel.currentPanel._loadEndpoint(endpoint);
            }
            return;
        }

        const panel = vscode.window.createWebviewPanel(
            RequestTesterPanel.viewType,
            'Soroban Pulse — Request Tester',
            column,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
                localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'media')],
            }
        );

        RequestTesterPanel.currentPanel = new RequestTesterPanel(panel, extensionUri, endpoint);
    }

    private constructor(
        panel: vscode.WebviewPanel,
        extensionUri: vscode.Uri,
        endpoint?: ApiEndpoint
    ) {
        this._panel = panel;
        this._extensionUri = extensionUri;

        this._panel.webview.html = this._buildHtml();

        // Handle messages from the webview
        this._panel.webview.onDidReceiveMessage(
            (msg: WebviewMessage) => this._handleMessage(msg),
            null,
            this._disposables
        );

        this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

        // Load initial endpoint once the webview is ready
        if (endpoint) {
            // Small delay to ensure the webview is initialized before sending
            setTimeout(() => this._loadEndpoint(endpoint), 300);
        }
    }

    private _loadEndpoint(endpoint: ApiEndpoint): void {
        const config = vscode.workspace.getConfiguration('sorobanpulse');
        const baseUrl = config.get<string>('baseUrl', 'http://localhost:3000');
        const apiKey = config.get<string>('apiKey', '');
        const adminApiKey = config.get<string>('adminApiKey', '');

        this._panel.webview.postMessage({
            type: 'loadEndpoint',
            endpoint,
            baseUrl,
            apiKey,
            adminApiKey,
        });
    }

    private async _handleMessage(msg: WebviewMessage): Promise<void> {
        switch (msg.type) {
            case 'sendRequest':
                await this._executeRequest(msg.config);
                break;
            case 'copyToClipboard':
                await vscode.env.clipboard.writeText(msg.text);
                vscode.window.showInformationMessage('Copied to clipboard.');
                break;
            case 'openSettings':
                await vscode.commands.executeCommand('workbench.action.openSettings', 'sorobanpulse');
                break;
        }
    }

    private async _executeRequest(config: RequestConfig): Promise<void> {
        this._panel.webview.postMessage({ type: 'loading' });

        const timeoutMs = vscode.workspace.getConfiguration('sorobanpulse').get<number>('timeoutMs', 10000);
        const start = Date.now();

        try {
            const result = await makeRequest(config, timeoutMs);
            result.durationMs = Date.now() - start;
            this._panel.webview.postMessage({ type: 'response', data: result });
        } catch (err) {
            this._panel.webview.postMessage({
                type: 'error',
                message: err instanceof Error ? err.message : String(err),
            });
        }
    }

    dispose(): void {
        RequestTesterPanel.currentPanel = undefined;
        this._panel.dispose();
        this._disposables.forEach(d => d.dispose());
    }

    // -----------------------------------------------------------------------
    // Webview HTML
    // -----------------------------------------------------------------------

    private _buildHtml(): string {
        const nonce = getNonce();
        return /* html */`<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy"
    content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Soroban Pulse — Request Tester</title>
  <style nonce="${nonce}">${STYLES}</style>
</head>
<body>
  <div class="toolbar">
    <span class="app-title">⚡ Soroban Pulse</span>
    <button class="btn-ghost" id="btnSettings" title="Open Settings">⚙</button>
  </div>

  <div class="pane" id="requestPane">
    <div class="url-bar">
      <span class="method-badge" id="methodBadge">GET</span>
      <input id="urlInput" class="url-input" type="text" placeholder="http://localhost:3000/v1/events" spellcheck="false">
      <button class="btn-primary" id="btnSend">Send</button>
    </div>

    <div class="tabs">
      <button class="tab active" data-tab="params">Params</button>
      <button class="tab" data-tab="headers">Headers</button>
      <button class="tab" data-tab="body">Body</button>
    </div>

    <div class="tab-content active" id="tab-params">
      <div id="queryParams"></div>
      <div id="pathParams"></div>
    </div>

    <div class="tab-content" id="tab-headers">
      <table class="kv-table" id="headersTable">
        <thead><tr><th>Key</th><th>Value</th><th></th></tr></thead>
        <tbody id="headerRows"></tbody>
      </table>
      <button class="btn-ghost btn-add" id="btnAddHeader">+ Add Header</button>
    </div>

    <div class="tab-content" id="tab-body">
      <textarea id="bodyInput" class="body-editor" placeholder='{"key": "value"}'></textarea>
    </div>
  </div>

  <div class="divider"></div>

  <div class="pane" id="responsePane">
    <div class="response-toolbar" id="responseToolbar" style="display:none">
      <span class="status-badge" id="statusBadge"></span>
      <span class="duration" id="durationBadge"></span>
      <div class="spacer"></div>
      <button class="btn-ghost" id="btnCopyResponse" title="Copy response">⧉</button>
    </div>

    <div id="emptyState" class="empty-state">
      <p>Select an endpoint from the explorer and press <strong>Send</strong>.</p>
    </div>

    <div id="loadingState" class="loading" style="display:none">Sending…</div>

    <div id="errorState" class="error-state" style="display:none">
      <span id="errorMsg"></span>
    </div>

    <div class="response-tabs" id="responseTabs" style="display:none">
      <button class="tab active" data-rtab="body">Body</button>
      <button class="tab" data-rtab="headers">Headers</button>
    </div>

    <div id="rtab-body" class="rtab-content active">
      <pre id="responseBody" class="response-body"></pre>
    </div>
    <div id="rtab-headers" class="rtab-content">
      <table class="resp-headers-table" id="responseHeaders"></table>
    </div>
  </div>

  <script nonce="${nonce}">${SCRIPT}</script>
</body>
</html>`;
    }
}

// ---------------------------------------------------------------------------
// HTTP client (Node built-ins only — no external deps)
// ---------------------------------------------------------------------------

function makeRequest(config: RequestConfig, timeoutMs: number): Promise<ResponseData> {
    return new Promise((resolve, reject) => {
        let url: URL;
        try {
            url = new URL(config.url);
        } catch {
            return reject(new Error(`Invalid URL: ${config.url}`));
        }

        const isHttps = url.protocol === 'https:';
        const lib = isHttps ? https : http;

        const body = config.body ? Buffer.from(config.body, 'utf-8') : undefined;

        const options: http.RequestOptions = {
            hostname: url.hostname,
            port: url.port || (isHttps ? 443 : 80),
            path: url.pathname + url.search,
            method: config.method,
            headers: {
                ...config.headers,
                ...(body ? { 'Content-Length': body.length } : {}),
            },
            timeout: timeoutMs,
        };

        const req = lib.request(options, (res) => {
            const chunks: Buffer[] = [];
            res.on('data', (chunk: Buffer) => chunks.push(chunk));
            res.on('end', () => {
                resolve({
                    status: res.statusCode ?? 0,
                    statusText: res.statusMessage ?? '',
                    headers: res.headers as Record<string, string>,
                    body: Buffer.concat(chunks).toString('utf-8'),
                    durationMs: 0,
                });
            });
        });

        req.on('timeout', () => { req.destroy(); reject(new Error(`Request timed out after ${timeoutMs}ms`)); });
        req.on('error', reject);

        if (body) { req.write(body); }
        req.end();
    });
}

function getNonce(): string {
    let text = '';
    const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    for (let i = 0; i < 32; i++) {
        text += possible.charAt(Math.floor(Math.random() * possible.length));
    }
    return text;
}

// ---------------------------------------------------------------------------
// Webview CSS
// ---------------------------------------------------------------------------

const STYLES = `
  :root {
    --gap: 8px;
    --radius: 4px;
    --font-mono: var(--vscode-editor-font-family, monospace);
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: var(--vscode-font-family);
    font-size: var(--vscode-font-size);
    color: var(--vscode-foreground);
    background: var(--vscode-editor-background);
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .toolbar {
    display: flex;
    align-items: center;
    padding: 6px 12px;
    background: var(--vscode-titleBar-activeBackground);
    border-bottom: 1px solid var(--vscode-panel-border);
    gap: var(--gap);
  }
  .app-title { font-weight: 600; font-size: 13px; flex: 1; }
  .pane { padding: 10px 12px; overflow-y: auto; flex: 1; min-height: 0; }
  .divider { height: 1px; background: var(--vscode-panel-border); flex-shrink: 0; }
  .url-bar { display: flex; gap: var(--gap); align-items: center; margin-bottom: 10px; }
  .method-badge {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    padding: 3px 8px;
    border-radius: var(--radius);
    background: var(--vscode-badge-background);
    color: var(--vscode-badge-foreground);
    white-space: nowrap;
  }
  .method-GET    { background: #1a5c2e; color: #4ec97b; }
  .method-POST   { background: #1b3a6b; color: #6db3ff; }
  .method-DELETE { background: #5c1a1a; color: #f97070; }
  .method-PUT    { background: #5c4a1a; color: #f0c040; }
  .method-PATCH  { background: #4a2e00; color: #e08020; }
  .url-input {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 5px 8px;
    background: var(--vscode-input-background);
    color: var(--vscode-input-foreground);
    border: 1px solid var(--vscode-input-border, transparent);
    border-radius: var(--radius);
    outline: none;
  }
  .url-input:focus { border-color: var(--vscode-focusBorder); }
  .btn-primary {
    padding: 5px 14px;
    background: var(--vscode-button-background);
    color: var(--vscode-button-foreground);
    border: none;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
  .btn-primary:hover { background: var(--vscode-button-hoverBackground); }
  .btn-ghost {
    background: transparent;
    border: none;
    color: var(--vscode-foreground);
    cursor: pointer;
    padding: 4px 6px;
    border-radius: var(--radius);
    opacity: 0.7;
  }
  .btn-ghost:hover { opacity: 1; background: var(--vscode-toolbar-hoverBackground); }
  .btn-add { margin-top: 6px; font-size: 11px; }
  .tabs, .response-tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--vscode-panel-border);
    margin-bottom: 10px;
  }
  .tab {
    background: transparent;
    border: none;
    color: var(--vscode-foreground);
    padding: 5px 12px;
    cursor: pointer;
    font-size: 12px;
    opacity: 0.7;
    border-bottom: 2px solid transparent;
  }
  .tab.active { opacity: 1; border-bottom-color: var(--vscode-focusBorder); }
  .tab-content, .rtab-content { display: none; }
  .tab-content.active, .rtab-content.active { display: block; }
  .section-label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    opacity: 0.6;
    margin: 10px 0 4px;
  }
  .param-row { display: flex; gap: var(--gap); align-items: center; margin-bottom: 4px; }
  .param-name { font-family: var(--font-mono); font-size: 11px; min-width: 120px; opacity: 0.8; }
  .param-input, .kv-input {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 4px 7px;
    background: var(--vscode-input-background);
    color: var(--vscode-input-foreground);
    border: 1px solid var(--vscode-input-border, transparent);
    border-radius: var(--radius);
    outline: none;
  }
  .param-input:focus, .kv-input:focus { border-color: var(--vscode-focusBorder); }
  .required-star { color: #f97070; margin-left: 2px; }
  .kv-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .kv-table th { text-align: left; opacity: 0.6; font-size: 10px; font-weight: 600;
                  text-transform: uppercase; padding-bottom: 4px; }
  .kv-table td { padding: 2px 4px 2px 0; }
  .kv-table td:last-child { width: 24px; }
  .btn-remove { background: transparent; border: none; color: #f97070; cursor: pointer; font-size: 14px; }
  .body-editor {
    width: 100%;
    min-height: 120px;
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 8px;
    background: var(--vscode-input-background);
    color: var(--vscode-input-foreground);
    border: 1px solid var(--vscode-input-border, transparent);
    border-radius: var(--radius);
    resize: vertical;
    outline: none;
  }
  .response-toolbar {
    display: flex;
    align-items: center;
    gap: var(--gap);
    margin-bottom: 8px;
  }
  .status-badge {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: var(--radius);
  }
  .status-2xx { background: #1a5c2e; color: #4ec97b; }
  .status-3xx { background: #1b3a6b; color: #6db3ff; }
  .status-4xx { background: #5c4a1a; color: #f0c040; }
  .status-5xx { background: #5c1a1a; color: #f97070; }
  .duration { font-size: 11px; opacity: 0.6; font-family: var(--font-mono); }
  .spacer { flex: 1; }
  .empty-state { text-align: center; opacity: 0.5; padding: 40px 0; font-size: 13px; }
  .loading { text-align: center; opacity: 0.6; padding: 30px 0; font-size: 13px; }
  .error-state {
    padding: 10px;
    background: #3c1a1a;
    border: 1px solid #7a3030;
    border-radius: var(--radius);
    color: #f97070;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .response-body {
    font-family: var(--font-mono);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-all;
    line-height: 1.5;
    tab-size: 2;
  }
  .resp-headers-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .resp-headers-table td {
    padding: 3px 8px 3px 0;
    vertical-align: top;
    border-bottom: 1px solid var(--vscode-panel-border);
  }
  .resp-headers-table td:first-child { font-family: var(--font-mono); opacity: 0.7; width: 40%; }
  .resp-headers-table td:last-child { font-family: var(--font-mono); word-break: break-all; }
`;

// ---------------------------------------------------------------------------
// Webview client-side JavaScript
// ---------------------------------------------------------------------------

const SCRIPT = `
  const vscode = acquireVsCodeApi();
  let currentEndpoint = null;

  // ── Tab switching ──────────────────────────────────────────────────────────
  document.querySelectorAll('.tab[data-tab]').forEach(btn => {
    btn.addEventListener('click', () => {
      const tab = btn.dataset.tab;
      document.querySelectorAll('.tab[data-tab]').forEach(b => b.classList.remove('active'));
      document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
      btn.classList.add('active');
      document.getElementById('tab-' + tab).classList.add('active');
    });
  });
  document.querySelectorAll('.tab[data-rtab]').forEach(btn => {
    btn.addEventListener('click', () => {
      const tab = btn.dataset.rtab;
      document.querySelectorAll('.tab[data-rtab]').forEach(b => b.classList.remove('active'));
      document.querySelectorAll('.rtab-content').forEach(c => c.classList.remove('active'));
      btn.classList.add('active');
      document.getElementById('rtab-' + tab).classList.add('active');
    });
  });

  // ── Settings ──────────────────────────────────────────────────────────────
  document.getElementById('btnSettings').addEventListener('click', () => {
    vscode.postMessage({ type: 'openSettings' });
  });

  // ── Add header row ─────────────────────────────────────────────────────────
  document.getElementById('btnAddHeader').addEventListener('click', () => addHeaderRow('', ''));

  function addHeaderRow(key, value, enabled = true) {
    const tbody = document.getElementById('headerRows');
    const tr = document.createElement('tr');
    tr.innerHTML =
      '<td><input class="kv-input header-key" value="' + esc(key) + '" placeholder="Header"></td>' +
      '<td><input class="kv-input header-val" value="' + esc(value) + '" placeholder="Value"></td>' +
      '<td><button class="btn-remove" title="Remove">×</button></td>';
    tr.querySelector('.btn-remove').addEventListener('click', () => tr.remove());
    tbody.appendChild(tr);
  }

  // ── Send request ───────────────────────────────────────────────────────────
  document.getElementById('btnSend').addEventListener('click', sendRequest);
  document.getElementById('urlInput').addEventListener('keydown', e => {
    if (e.key === 'Enter') sendRequest();
  });

  function sendRequest() {
    let url = document.getElementById('urlInput').value.trim();

    // Substitute path params
    document.querySelectorAll('.path-param-input').forEach(inp => {
      const name = inp.dataset.name;
      const val = inp.value.trim();
      if (val) url = url.replace('{' + name + '}', encodeURIComponent(val));
    });

    // Append query params
    const qParams = [];
    document.querySelectorAll('.query-param-input').forEach(inp => {
      const name = inp.dataset.name;
      const val = inp.value.trim();
      if (val) qParams.push(encodeURIComponent(name) + '=' + encodeURIComponent(val));
    });
    if (qParams.length) {
      url += (url.includes('?') ? '&' : '?') + qParams.join('&');
    }

    // Collect headers
    const headers = {};
    document.querySelectorAll('#headerRows tr').forEach(tr => {
      const k = tr.querySelector('.header-key').value.trim();
      const v = tr.querySelector('.header-val').value.trim();
      if (k) headers[k] = v;
    });

    const method = currentEndpoint?.method ?? 'GET';
    const body = document.getElementById('bodyInput').value.trim() || undefined;
    if (body && !headers['Content-Type']) {
      headers['Content-Type'] = 'application/json';
    }

    vscode.postMessage({ type: 'sendRequest', config: { method, url, headers, body } });
  }

  // ── Copy response ──────────────────────────────────────────────────────────
  document.getElementById('btnCopyResponse').addEventListener('click', () => {
    const text = document.getElementById('responseBody').textContent;
    vscode.postMessage({ type: 'copyToClipboard', text });
  });

  // ── Messages from extension ────────────────────────────────────────────────
  window.addEventListener('message', ({ data }) => {
    switch (data.type) {
      case 'loadEndpoint': loadEndpoint(data); break;
      case 'loading':       showLoading();      break;
      case 'response':      showResponse(data.data); break;
      case 'error':         showError(data.message); break;
    }
  });

  function loadEndpoint({ endpoint, baseUrl, apiKey, adminApiKey }) {
    currentEndpoint = endpoint;

    // Method badge
    const badge = document.getElementById('methodBadge');
    badge.textContent = endpoint.method;
    badge.className = 'method-badge method-' + endpoint.method;

    // URL
    document.getElementById('urlInput').value = baseUrl.replace(/\\/$/, '') + endpoint.path;

    // Body
    document.getElementById('bodyInput').value = endpoint.bodyExample ?? '';

    // Params
    const qEl = document.getElementById('queryParams');
    const pEl = document.getElementById('pathParams');
    qEl.innerHTML = '';
    pEl.innerHTML = '';

    const pathParams = (endpoint.params ?? []).filter(p => p.in === 'path');
    const queryParams = (endpoint.params ?? []).filter(p => p.in === 'query');

    if (pathParams.length) {
      pEl.innerHTML = '<div class="section-label">Path parameters</div>';
      pathParams.forEach(p => pEl.appendChild(paramRow(p, 'path-param-input')));
    }
    if (queryParams.length) {
      qEl.innerHTML = '<div class="section-label">Query parameters</div>';
      queryParams.forEach(p => qEl.appendChild(paramRow(p, 'query-param-input')));
    }
    if (!pathParams.length && !queryParams.length) {
      qEl.innerHTML = '<p style="opacity:0.5;font-size:12px;padding:10px 0">No parameters.</p>';
    }

    // Headers
    document.getElementById('headerRows').innerHTML = '';
    if (endpoint.auth === 'api-key') {
      addHeaderRow('x-api-key', apiKey);
    } else if (endpoint.auth === 'admin-key') {
      addHeaderRow('x-api-key', adminApiKey);
    }

    // Reset response pane
    hideResponse();
  }

  function paramRow(param, cls) {
    const div = document.createElement('div');
    div.className = 'param-row';
    const star = param.required ? '<span class="required-star">*</span>' : '';
    div.innerHTML =
      '<span class="param-name">' + esc(param.name) + star + '</span>' +
      '<input class="param-input ' + cls + '" data-name="' + esc(param.name) + '"' +
      ' placeholder="' + esc(param.example ?? '') + '">';
    return div;
  }

  function showLoading() {
    document.getElementById('emptyState').style.display = 'none';
    document.getElementById('errorState').style.display = 'none';
    document.getElementById('responseToolbar').style.display = 'none';
    document.getElementById('responseTabs').style.display = 'none';
    document.getElementById('loadingState').style.display = 'block';
    document.getElementById('rtab-body').classList.remove('active');
    document.getElementById('rtab-headers').classList.remove('active');
  }

  function showResponse(data) {
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('errorState').style.display = 'none';
    document.getElementById('emptyState').style.display = 'none';
    document.getElementById('responseToolbar').style.display = 'flex';
    document.getElementById('responseTabs').style.display = 'flex';
    document.getElementById('rtab-body').classList.add('active');

    // Status badge
    const badge = document.getElementById('statusBadge');
    const cls = data.status < 300 ? '2xx' : data.status < 400 ? '3xx' : data.status < 500 ? '4xx' : '5xx';
    badge.className = 'status-badge status-' + cls;
    badge.textContent = data.status + ' ' + data.statusText;

    document.getElementById('durationBadge').textContent = data.durationMs + ' ms';

    // Body — pretty-print JSON
    let body = data.body;
    try { body = JSON.stringify(JSON.parse(body), null, 2); } catch {}
    document.getElementById('responseBody').textContent = body;

    // Headers
    const tbl = document.getElementById('responseHeaders');
    tbl.innerHTML = '';
    Object.entries(data.headers).forEach(([k, v]) => {
      const tr = document.createElement('tr');
      tr.innerHTML = '<td>' + esc(k) + '</td><td>' + esc(String(v)) + '</td>';
      tbl.appendChild(tr);
    });
  }

  function showError(msg) {
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('emptyState').style.display = 'none';
    document.getElementById('errorState').style.display = 'block';
    document.getElementById('responseToolbar').style.display = 'none';
    document.getElementById('responseTabs').style.display = 'none';
    document.getElementById('errorMsg').textContent = msg;
  }

  function hideResponse() {
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('errorState').style.display = 'none';
    document.getElementById('responseToolbar').style.display = 'none';
    document.getElementById('responseTabs').style.display = 'none';
    document.getElementById('emptyState').style.display = 'block';
    document.getElementById('rtab-body').classList.remove('active');
    document.getElementById('rtab-headers').classList.remove('active');
  }

  function esc(str) {
    return String(str)
      .replace(/&/g, '&amp;').replace(/</g, '&lt;')
      .replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }
`;
