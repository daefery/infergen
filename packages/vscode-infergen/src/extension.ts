import * as vscode from 'vscode';
import * as path from 'path';
import { CatalogManager } from './catalog';
import { resolveConfig } from './config';
import { DiagnosticsManager } from './diagnostics';
import { InfergenCodeLensProvider } from './codelens';
import { InfergenCompletionProvider } from './completion';
import { InfergenHoverProvider } from './hover';
import { InfergenDefinitionProvider } from './definition';

const SUPPORTED_LANGUAGES = [
  'typescript',
  'typescriptreact',
  'javascript',
  'javascriptreact',
  'python',
  'go',
  'ruby',
];

export function activate(context: vscode.ExtensionContext): void {
  const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (!workspaceRoot) {
    return;
  }

  const { catalogPath } = resolveConfig(workspaceRoot);
  const settings = vscode.workspace.getConfiguration('infergen');
  const binaryPath = settings.get<string>('binaryPath', 'infergen');

  const catalogManager = new CatalogManager(catalogPath);
  context.subscriptions.push(catalogManager);

  if (settings.get<boolean>('enableDiagnostics', true)) {
    const diag = new DiagnosticsManager(workspaceRoot, binaryPath, catalogPath);
    context.subscriptions.push(diag);
  }

  if (settings.get<boolean>('enableCodeLens', true)) {
    const codeLens = new InfergenCodeLensProvider(catalogManager, workspaceRoot);
    context.subscriptions.push(
      vscode.languages.registerCodeLensProvider(
        SUPPORTED_LANGUAGES.map((l) => ({ language: l })),
        codeLens
      ),
      codeLens
    );
  }

  const completion = new InfergenCompletionProvider(catalogManager);
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      ['typescript', 'typescriptreact', 'javascript', 'javascriptreact'].map((l) => ({
        language: l,
      })),
      completion,
      '.'
    ),
    completion
  );

  const hover = new InfergenHoverProvider(catalogManager);
  context.subscriptions.push(
    vscode.languages.registerHoverProvider(
      SUPPORTED_LANGUAGES.map((l) => ({ language: l })),
      hover
    ),
    hover
  );

  const definition = new InfergenDefinitionProvider(catalogManager, workspaceRoot);
  context.subscriptions.push(
    vscode.languages.registerDefinitionProvider(
      SUPPORTED_LANGUAGES.map((l) => ({ language: l })),
      definition
    ),
    definition
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('infergen.runScan', () =>
      runInTerminal(workspaceRoot, `${binaryPath} scan`)
    ),
    vscode.commands.registerCommand('infergen.jumpToTrigger', (eventId: string) =>
      jumpToTrigger(catalogManager, workspaceRoot, eventId)
    ),
    vscode.commands.registerCommand('infergen.approveEvent', (eventId: string) =>
      runInTerminal(workspaceRoot, `${binaryPath} review approve ${eventId}`)
    ),
    vscode.commands.registerCommand('infergen.ignoreEvent', (eventId: string) =>
      runInTerminal(workspaceRoot, `${binaryPath} review ignore ${eventId}`)
    ),
  );
}

export function deactivate(): void {}

function runInTerminal(cwd: string, command: string): void {
  const terminal = vscode.window.createTerminal({ name: 'Infergen', cwd });
  terminal.sendText(command);
  terminal.show();
}

function jumpToTrigger(
  catalogManager: CatalogManager,
  workspaceRoot: string,
  eventId: string,
): void {
  const catalog = catalogManager.getCatalog();
  if (!catalog) {
    return;
  }
  const entry = catalog.events.find((e) => e.id === eventId);
  if (!entry || entry.provenance.length === 0) {
    return;
  }
  const prov = entry.provenance[0];
  const absPath = path.isAbsolute(prov.sourcePath)
    ? prov.sourcePath
    : path.join(workspaceRoot, prov.sourcePath);
  const lineIdx = Math.max(0, (prov.line ?? 1) - 1);
  void vscode.workspace.openTextDocument(absPath).then((doc) =>
    vscode.window.showTextDocument(doc, {
      selection: new vscode.Range(lineIdx, 0, lineIdx, 0),
    })
  );
}
