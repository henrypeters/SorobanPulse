import * as vscode from 'vscode';
import { ApiExplorerProvider, EndpointItem } from './apiExplorer';
import { RequestTesterPanel } from './requestTester';
import { ApiEndpoint } from './types';

export function activate(context: vscode.ExtensionContext): void {
    const explorer = new ApiExplorerProvider();

    // Tree view
    const treeView = vscode.window.createTreeView('sorobanpulse.apiExplorer', {
        treeDataProvider: explorer,
        showCollapseAll: true,
    });

    // Search/filter box above the tree
    const searchBox = vscode.window.createInputBox();
    searchBox.placeholder = 'Filter endpoints…';
    searchBox.onDidChangeValue(v => explorer.setFilter(v));

    // Commands
    context.subscriptions.push(
        treeView,

        vscode.commands.registerCommand('sorobanpulse.refreshExplorer', () => {
            explorer.setFilter('');
            explorer.refresh();
        }),

        vscode.commands.registerCommand('sorobanpulse.openRequestTester', (endpoint?: ApiEndpoint) => {
            RequestTesterPanel.open(context.extensionUri, endpoint);
        }),

        vscode.commands.registerCommand('sorobanpulse.copyUrl', async (item?: EndpointItem) => {
            if (!item) { return; }
            const base = vscode.workspace.getConfiguration('sorobanpulse').get<string>('baseUrl', 'http://localhost:3000');
            const url = base.replace(/\/$/, '') + item.endpoint.path;
            await vscode.env.clipboard.writeText(url);
            vscode.window.showInformationMessage(`Copied: ${url}`);
        }),

        vscode.commands.registerCommand('sorobanpulse.openSettings', () => {
            vscode.commands.executeCommand('workbench.action.openSettings', 'sorobanpulse');
        }),
    );
}

export function deactivate(): void {
    // Nothing to clean up — disposables handled via context.subscriptions
}
