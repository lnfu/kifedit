import * as path from "node:path";
import * as vscode from "vscode";
import { LanguageClient, Location } from "vscode-languageclient/node";

interface Support {
  text: string;
  location: Location;
}

interface Row {
  values: string[];
  support: Support[];
}

type QueryOutcome =
  | { status: "answers"; columns: string[]; rows: Row[] }
  | { status: "needsProver"; message: string }
  | { status: "error"; message: string };

type Item = { row: Row } | { support: Support };

class QueryResults implements vscode.TreeDataProvider<Item> {
  private changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;
  private columns: string[] = [];
  private rows: Row[] = [];

  constructor(private client: LanguageClient) {}

  show(columns: string[], rows: Row[]) {
    this.columns = columns;
    this.rows = rows;
    this.changed.fire();
  }

  getChildren(item?: Item): Item[] {
    if (!item) return this.rows.map((row) => ({ row }));
    if ("row" in item) return item.row.support.map((support) => ({ support }));
    return [];
  }

  getTreeItem(item: Item): vscode.TreeItem {
    if ("row" in item) {
      const label = this.columns.length
        ? this.columns.map((c, i) => `${c} = ${item.row.values[i]}`).join(", ")
        : "true";
      return new vscode.TreeItem(
        label,
        item.row.support.length
          ? vscode.TreeItemCollapsibleState.Collapsed
          : vscode.TreeItemCollapsibleState.None,
      );
    }
    const location = this.client.protocol2CodeConverter.asLocation(item.support.location);
    const tree = new vscode.TreeItem(item.support.text);
    tree.description = `${path.basename(location.uri.fsPath)}:${location.range.start.line + 1}`;
    tree.iconPath = new vscode.ThemeIcon("symbol-misc");
    tree.command = {
      command: "vscode.open",
      title: "Open Formula",
      arguments: [location.uri, { selection: location.range, preview: true }],
    };
    return tree;
  }
}

export function registerQuery(context: vscode.ExtensionContext, client: LanguageClient) {
  const results = new QueryResults(client);
  let last = "";

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("kifedit.query", results),
    vscode.commands.registerCommand("kifedit.runQuery", async () => {
      const editor = vscode.window.activeTextEditor;
      const selected = editor && !editor.selection.isEmpty ? editor.document.getText(editor.selection) : "";
      const query = await vscode.window.showInputBox({
        prompt: "Query, for example (instance ?X Camera)",
        value: selected || last,
      });
      if (!query) return;
      last = query;

      const outcome = await client.sendRequest<QueryOutcome>("kifedit/query", { query });
      if (outcome.status !== "answers") {
        vscode.window.showWarningMessage(outcome.message);
        return;
      }
      results.show(outcome.columns, outcome.rows);
      await vscode.commands.executeCommand("kifedit.query.focus");
      if (outcome.rows.length === 0) {
        vscode.window.showInformationMessage("The query has no answers.");
      }
    }),
  );
}
