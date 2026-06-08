import * as vscode from 'vscode';
import * as path from 'path';
import { CatalogManager } from './catalog';

/** Shows code lens above each provenance location in the catalog. */
export class InfergenCodeLensProvider
  implements vscode.CodeLensProvider, vscode.Disposable
{
  private readonly changeEmitter = new vscode.EventEmitter<void>();
  readonly onDidChangeCodeLenses = this.changeEmitter.event;
  private readonly disposables: vscode.Disposable[] = [];

  constructor(
    private readonly catalogManager: CatalogManager,
    private readonly workspaceRoot: string,
  ) {
    this.disposables.push(
      catalogManager.onDidChange(() => this.changeEmitter.fire())
    );
  }

  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const catalog = this.catalogManager.getCatalog();
    if (!catalog) {
      return [];
    }

    const docPath = path.normalize(document.uri.fsPath);
    const lenses: vscode.CodeLens[] = [];

    for (const entry of catalog.events) {
      if (entry.status === 'ignored') {
        continue;
      }

      for (const prov of entry.provenance) {
        const absSource = path.isAbsolute(prov.sourcePath)
          ? prov.sourcePath
          : path.join(this.workspaceRoot, prov.sourcePath);

        if (path.normalize(absSource) !== docPath) {
          continue;
        }

        const lineIdx = Math.max(0, (prov.line ?? 1) - 1);
        const range = new vscode.Range(lineIdx, 0, lineIdx, 0);

        const statusIcon = entry.status === 'approved' ? '$(check)' : '$(warning)';
        lenses.push(
          new vscode.CodeLens(range, {
            title: `${statusIcon} Infergen: ${entry.name} · ${entry.status}`,
            command: 'infergen.jumpToTrigger',
            arguments: [entry.id],
            tooltip: entry.description || `Event: ${entry.name}`,
          })
        );

        if (entry.status === 'proposed') {
          lenses.push(
            new vscode.CodeLens(range, {
              title: '$(check) Approve',
              command: 'infergen.approveEvent',
              arguments: [entry.id],
            }),
            new vscode.CodeLens(range, {
              title: '$(eye-closed) Ignore',
              command: 'infergen.ignoreEvent',
              arguments: [entry.id],
            })
          );
        }
      }
    }

    return lenses;
  }

  dispose(): void {
    this.changeEmitter.dispose();
    this.disposables.forEach((d) => d.dispose());
  }
}
