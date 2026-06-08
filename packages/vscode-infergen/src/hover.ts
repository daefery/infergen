import * as vscode from 'vscode';
import { CatalogManager, matchesEventName } from './catalog';

/** Shows catalog entry details when hovering over an event name. */
export class InfergenHoverProvider
  implements vscode.HoverProvider, vscode.Disposable
{
  constructor(private readonly catalogManager: CatalogManager) {}

  provideHover(
    document: vscode.TextDocument,
    position: vscode.Position,
  ): vscode.Hover | null {
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
    if (!entry) {
      return null;
    }

    const md = new vscode.MarkdownString();
    const statusBadge =
      entry.status === 'approved' ? '✅' : entry.status === 'proposed' ? '⚠️' : '🚫';
    md.appendMarkdown(`**${entry.name}** ${statusBadge} _(${entry.kind})_\n\n`);

    if (entry.description) {
      md.appendMarkdown(`${entry.description}\n\n`);
    }

    if (entry.properties.length > 0) {
      md.appendMarkdown('| Property | Type | Required | PII |\n');
      md.appendMarkdown('|----------|------|----------|-----|\n');
      for (const p of entry.properties) {
        md.appendMarkdown(
          `| \`${p.name}\` | ${p.type ?? 'unknown'} | ${p.required ? 'yes' : 'no'} | ${p.pii ? '⚠️' : 'no'} |\n`
        );
      }
      md.appendMarkdown('\n');
    }

    if (entry.provenance.length > 0) {
      const sources = entry.provenance
        .map((p) => `\`${p.sourcePath}\`${p.line ? `:${p.line}` : ''}`)
        .join(', ');
      md.appendMarkdown(`**Source:** ${sources}\n`);
    }

    md.isTrusted = true;
    return new vscode.Hover(md);
  }

  dispose(): void {}
}
