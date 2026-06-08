import * as vscode from 'vscode';
import { CatalogManager, toCamelCase } from './catalog';

/** Provides `track.X` event completions and property completions inside call arguments. */
export class InfergenCompletionProvider
  implements vscode.CompletionItemProvider, vscode.Disposable
{
  constructor(private readonly catalogManager: CatalogManager) {}

  provideCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position,
    _token: vscode.CancellationToken,
    _context: vscode.CompletionContext,
  ): vscode.CompletionItem[] {
    const lineText = document.lineAt(position).text;
    const textBefore = lineText.slice(0, position.character);

    if (/\btrack\.$/.test(textBefore)) {
      return this.buildEventCompletions();
    }

    const eventName = extractEventNameFromContext(document, position);
    if (eventName) {
      return this.buildPropertyCompletions(eventName);
    }

    return [];
  }

  private buildEventCompletions(): vscode.CompletionItem[] {
    return this.catalogManager.getApprovedEvents().map((entry) => {
      const camel = toCamelCase(entry.name);
      const item = new vscode.CompletionItem(camel, vscode.CompletionItemKind.Method);
      item.detail = `(${entry.kind}) ${Math.round(entry.confidence * 100)}% confidence`;
      const propNames = entry.properties.map((p) => p.name).join(', ');
      item.documentation = new vscode.MarkdownString(
        (entry.description || `Track \`${entry.name}\` event.`) +
          (propNames ? `\n\n**Properties:** ${propNames}` : '')
      );
      item.insertText = new vscode.SnippetString(`${camel}($\{1:{}\})`);
      return item;
    });
  }

  private buildPropertyCompletions(eventName: string): vscode.CompletionItem[] {
    const catalog = this.catalogManager.getCatalog();
    if (!catalog) {
      return [];
    }
    const entry = catalog.events.find(
      (e) => e.name === eventName || toCamelCase(e.name) === eventName
    );
    if (!entry) {
      return [];
    }
    return entry.properties.map((prop) => {
      const item = new vscode.CompletionItem(prop.name, vscode.CompletionItemKind.Property);
      item.detail = `${prop.type ?? 'unknown'}${prop.required ? ' (required)' : ''}${prop.pii ? ' · PII' : ''}`;
      if (prop.pii) {
        item.documentation = new vscode.MarkdownString(
          '⚠️ **PII** — contains personally identifiable information.'
        );
      }
      return item;
    });
  }

  dispose(): void {}
}

/**
 * Scan backwards up to 3 lines to find a `trackEventName(` pattern.
 * Returns the snake_case event name derived from the function suffix, or null.
 */
function extractEventNameFromContext(
  document: vscode.TextDocument,
  position: vscode.Position,
): string | null {
  const maxLookback = 3;
  const startLine = Math.max(0, position.line - maxLookback);
  for (let i = position.line; i >= startLine; i--) {
    const text = document.lineAt(i).text;
    const match = text.match(/\btrack([A-Z][a-zA-Z]*)\s*\(/);
    if (match) {
      return match[1]
        .replace(/([A-Z])/g, '_$1')
        .toLowerCase()
        .replace(/^_/, '');
    }
  }
  return null;
}
