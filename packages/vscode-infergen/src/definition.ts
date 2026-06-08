import * as vscode from 'vscode';
import * as path from 'path';
import { CatalogManager, matchesEventName } from './catalog';

/** Provides jump-to-trigger: navigates from an event name to its source provenance. */
export class InfergenDefinitionProvider
  implements vscode.DefinitionProvider, vscode.Disposable
{
  constructor(
    private readonly catalogManager: CatalogManager,
    private readonly workspaceRoot: string,
  ) {}

  provideDefinition(
    document: vscode.TextDocument,
    position: vscode.Position,
  ): vscode.Location[] | null {
    const word = document.getText(
      document.getWordRangeAtPosition(position, /[\w_]+/)
    );
    if (!word) {
      return null;
    }

    const catalog = this.catalogManager.getCatalog();
    if (!catalog) {
      return null;
    }

    const entry = catalog.events.find((e) => matchesEventName(word, e.name));
    if (!entry || entry.provenance.length === 0) {
      return null;
    }

    const locations = entry.provenance
      .filter((p) => p.sourcePath.length > 0)
      .map((p) => {
        const absPath = path.isAbsolute(p.sourcePath)
          ? p.sourcePath
          : path.join(this.workspaceRoot, p.sourcePath);
        const lineIdx = Math.max(0, (p.line ?? 1) - 1);
        const range = new vscode.Range(lineIdx, 0, lineIdx, 0);
        return new vscode.Location(vscode.Uri.file(absPath), range);
      });

    return locations.length > 0 ? locations : null;
  }

  dispose(): void {}
}
