import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';

interface CheckEventRef {
  id: string;
  name: string;
  source_path?: string;
  line?: number;
}

interface CheckViolation {
  event_name: string;
  message: string;
  suggestion?: string;
}

interface CheckReport {
  ok: boolean;
  issue_count: number;
  new_untracked: CheckEventRef[];
  unreviewed: CheckEventRef[];
  violations: CheckViolation[];
}

/** Runs `infergen check --json` on save and translates output to VS Code diagnostics. */
export class DiagnosticsManager implements vscode.Disposable {
  private readonly collection: vscode.DiagnosticCollection;
  private readonly sourceWatcher: vscode.FileSystemWatcher;
  private readonly catalogWatcher: vscode.FileSystemWatcher;
  private debounceTimer: ReturnType<typeof setTimeout> | undefined;
  private readonly disposables: vscode.Disposable[] = [];

  constructor(
    private readonly workspaceRoot: string,
    private readonly binaryPath: string,
    private readonly catalogPath: string,
  ) {
    this.collection = vscode.languages.createDiagnosticCollection('infergen');

    this.sourceWatcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(workspaceRoot, '**/*.{ts,tsx,js,jsx,py}')
    );
    this.sourceWatcher.onDidChange(() => this.scheduleRefresh());
    this.sourceWatcher.onDidCreate(() => this.scheduleRefresh());

    this.catalogWatcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(path.dirname(catalogPath), path.basename(catalogPath))
    );
    this.catalogWatcher.onDidChange(() => this.scheduleRefresh());
    this.disposables.push(this.catalogWatcher);

    this.scheduleRefresh();
  }

  private scheduleRefresh(): void {
    clearTimeout(this.debounceTimer);
    this.debounceTimer = setTimeout(() => void this.refresh(), 600);
  }

  async refresh(): Promise<void> {
    let stdout = '';
    try {
      stdout = await runCommand(this.binaryPath, ['check', '--json'], this.workspaceRoot);
    } catch (err: unknown) {
      if (err instanceof ProcessError) {
        stdout = err.stdout;
      } else {
        return;
      }
    }

    let report: CheckReport;
    try {
      report = JSON.parse(stdout) as CheckReport;
    } catch {
      return;
    }

    this.collection.clear();
    const byFile = new Map<string, vscode.Diagnostic[]>();

    const addDiag = (
      sourcePath: string | undefined,
      line: number | undefined,
      message: string,
      severity: vscode.DiagnosticSeverity
    ): void => {
      const filePath = sourcePath
        ? path.join(this.workspaceRoot, sourcePath)
        : this.catalogPath;
      const lineIdx = Math.max(0, (line ?? 1) - 1);
      const range = new vscode.Range(lineIdx, 0, lineIdx, 999);
      const diag = new vscode.Diagnostic(range, message, severity);
      diag.source = 'infergen';
      const list = byFile.get(filePath) ?? [];
      list.push(diag);
      byFile.set(filePath, list);
    };

    for (const item of report.new_untracked) {
      addDiag(
        item.source_path,
        item.line,
        `Untracked moment "${item.name}" — run \`infergen scan\` to add to catalog`,
        vscode.DiagnosticSeverity.Warning
      );
    }

    for (const v of report.violations) {
      const msg = v.suggestion
        ? `Naming violation: "${v.event_name}" → "${v.suggestion}"`
        : `Naming violation: ${v.message}`;
      addDiag(undefined, undefined, msg, vscode.DiagnosticSeverity.Error);
    }

    for (const [filePath, diags] of byFile) {
      this.collection.set(vscode.Uri.file(filePath), diags);
    }
  }

  dispose(): void {
    clearTimeout(this.debounceTimer);
    this.sourceWatcher.dispose();
    this.collection.dispose();
    this.disposables.forEach((d) => d.dispose());
  }
}

class ProcessError extends Error {
  constructor(
    public readonly stdout: string,
    public readonly stderr: string
  ) {
    super(`Process failed: ${stderr}`);
  }
}

function runCommand(bin: string, args: string[], cwd: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const proc = cp.spawn(bin, args, { cwd });
    let stdout = '';
    let stderr = '';
    proc.stdout.on('data', (d: Buffer) => { stdout += d.toString(); });
    proc.stderr.on('data', (d: Buffer) => { stderr += d.toString(); });
    proc.on('close', (code) => {
      if (code === 0) {
        resolve(stdout);
      } else {
        reject(new ProcessError(stdout, stderr));
      }
    });
    proc.on('error', (err) => reject(err));
  });
}
