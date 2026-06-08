// Minimal vscode API mock for unit tests (no VS Code process needed).

export const EventEmitter = class {
  event = (_listener: unknown) => ({ dispose: () => {} });
  fire(_value: unknown) {}
  dispose() {}
};

export const workspace = {
  createFileSystemWatcher: () => ({
    onDidChange: () => ({ dispose: () => {} }),
    onDidCreate: () => ({ dispose: () => {} }),
    onDidDelete: () => ({ dispose: () => {} }),
    dispose: () => {},
  }),
  workspaceFolders: [],
  getConfiguration: () => ({
    get: (_key: string, def: unknown) => def,
  }),
};

export const RelativePattern = class {
  constructor(public base: unknown, public pattern: string) {}
};

export class Uri {
  static file(fsPath: string) {
    return { fsPath, scheme: 'file', toString: () => `file://${fsPath}` };
  }
}

export const Range = class {
  constructor(
    public startLine: number,
    public startChar: number,
    public endLine: number,
    public endChar: number,
  ) {}
};

export const DiagnosticSeverity = { Warning: 1, Error: 0, Information: 2, Hint: 3 };
export class Diagnostic {
  source?: string;
  constructor(
    public range: unknown,
    public message: string,
    public severity: number,
  ) {}
}

export const languages = {
  createDiagnosticCollection: () => ({
    set: () => {},
    clear: () => {},
    dispose: () => {},
  }),
  registerCodeLensProvider: () => ({ dispose: () => {} }),
  registerCompletionItemProvider: () => ({ dispose: () => {} }),
  registerHoverProvider: () => ({ dispose: () => {} }),
  registerDefinitionProvider: () => ({ dispose: () => {} }),
};

export const window = {
  createTerminal: () => ({ sendText: () => {}, show: () => {} }),
};

export const commands = {
  registerCommand: () => ({ dispose: () => {} }),
};

export const CompletionItemKind = { Method: 1, Property: 9 };
export class CompletionItem {
  detail?: string;
  documentation?: unknown;
  insertText?: unknown;
  constructor(public label: string, public kind?: number) {}
}

export class MarkdownString {
  isTrusted?: boolean;
  constructor(public value = '') {}
  appendMarkdown(s: string) { this.value += s; return this; }
  appendText(s: string) { this.value += s; return this; }
}

export class SnippetString {
  constructor(public value: string) {}
}

export class CodeLens {
  constructor(public range: unknown, public command?: unknown) {}
}

export class Hover {
  constructor(public contents: unknown) {}
}

export class Location {
  constructor(public uri: unknown, public range: unknown) {}
}
