import * as vscode from 'vscode';
import { API_GROUPS } from './apiData';
import { ApiEndpoint, EndpointGroup } from './types';

// ---------------------------------------------------------------------------
// Tree items
// ---------------------------------------------------------------------------

export class GroupItem extends vscode.TreeItem {
    constructor(public readonly group: EndpointGroup) {
        super(group.label, vscode.TreeItemCollapsibleState.Expanded);
        this.contextValue = 'group';
        this.iconPath = new vscode.ThemeIcon('folder');
    }
}

export class EndpointItem extends vscode.TreeItem {
    constructor(public readonly endpoint: ApiEndpoint) {
        super(endpoint.path, vscode.TreeItemCollapsibleState.None);
        this.contextValue = 'endpoint';
        this.description = endpoint.description;
        this.tooltip = new vscode.MarkdownString(
            `**${endpoint.method}** \`${endpoint.path}\`\n\n${endpoint.description}` +
            (endpoint.streaming ? '\n\n_Streaming (SSE)_' : '') +
            (endpoint.deprecated ? '\n\n⚠️ _Deprecated_' : '')
        );
        this.iconPath = methodIcon(endpoint.method);
        this.command = {
            command: 'sorobanpulse.openRequestTester',
            title: 'Open in Request Tester',
            arguments: [endpoint],
        };
    }
}

function methodIcon(method: string): vscode.ThemeIcon {
    const colors: Record<string, string> = {
        GET: 'charts.green',
        POST: 'charts.blue',
        DELETE: 'charts.red',
        PUT: 'charts.yellow',
        PATCH: 'charts.orange',
    };
    return new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor(colors[method] ?? 'foreground'));
}

// ---------------------------------------------------------------------------
// Tree data provider
// ---------------------------------------------------------------------------

type TreeNode = GroupItem | EndpointItem;

export class ApiExplorerProvider implements vscode.TreeDataProvider<TreeNode> {
    private readonly _onDidChangeTreeData = new vscode.EventEmitter<TreeNode | undefined | void>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    private filter = '';

    refresh(): void {
        this._onDidChangeTreeData.fire();
    }

    setFilter(text: string): void {
        this.filter = text.toLowerCase();
        this.refresh();
    }

    getTreeItem(element: TreeNode): vscode.TreeItem {
        return element;
    }

    getChildren(element?: TreeNode): TreeNode[] {
        if (!element) {
            return this.filteredGroups().map(g => new GroupItem(g));
        }
        if (element instanceof GroupItem) {
            return this.filteredEndpoints(element.group).map(e => new EndpointItem(e));
        }
        return [];
    }

    private filteredGroups(): EndpointGroup[] {
        if (!this.filter) {
            return API_GROUPS;
        }
        return API_GROUPS
            .map(g => ({ ...g, endpoints: g.endpoints.filter(e => this.matchesFilter(e)) }))
            .filter(g => g.endpoints.length > 0);
    }

    private filteredEndpoints(group: EndpointGroup): ApiEndpoint[] {
        if (!this.filter) {
            return group.endpoints;
        }
        return group.endpoints.filter(e => this.matchesFilter(e));
    }

    private matchesFilter(e: ApiEndpoint): boolean {
        return (
            e.path.toLowerCase().includes(this.filter) ||
            e.description.toLowerCase().includes(this.filter) ||
            e.method.toLowerCase().includes(this.filter)
        );
    }
}
