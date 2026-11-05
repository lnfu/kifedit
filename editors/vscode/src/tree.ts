import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

interface TreeNode {
  name: string;
  kind: "class" | "member";
  hasChildren: boolean;
  otherParents: number;
}

export class TaxonomyTree implements vscode.TreeDataProvider<TreeNode> {
  private changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;

  constructor(
    private client: LanguageClient,
    private view: string,
  ) {}

  refresh() {
    this.changed.fire();
  }

  getChildren(node?: TreeNode): Thenable<TreeNode[]> {
    return this.client.sendRequest<TreeNode[]>("kifedit/treeChildren", {
      view: this.view,
      node: node?.name ?? null,
    });
  }

  // 同一個詞彙可能出現在多個父節點下，所以不設定 TreeItem.id
  getTreeItem(node: TreeNode): vscode.TreeItem {
    const item = new vscode.TreeItem(
      node.name,
      node.hasChildren
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None,
    );
    item.iconPath = new vscode.ThemeIcon(node.kind === "class" ? "symbol-class" : "symbol-constant");
    if (node.otherParents > 0) {
      item.description = `+${node.otherParents} parents`;
    }
    item.command = { command: "kifedit.openTermPage", title: "Open Term Page", arguments: [node.name] };
    return item;
  }
}

export function registerTrees(context: vscode.ExtensionContext, client: LanguageClient) {
  const trees = ["classes", "relations", "attributes"].map((view) => {
    const tree = new TaxonomyTree(client, view);
    context.subscriptions.push(vscode.window.registerTreeDataProvider(`kifedit.${view}`, tree));
    return tree;
  });

  const refreshAll = () => trees.forEach((t) => t.refresh());

  // 停止輸入或檔案變動後才更新，server 這時已收到修改
  let timer: NodeJS.Timeout | undefined;
  const refreshSoon = () => {
    clearTimeout(timer);
    timer = setTimeout(refreshAll, 500);
  };
  const watcher = vscode.workspace.createFileSystemWatcher("**/{*.kif,.kifignore,project.toml}");
  watcher.onDidCreate(refreshSoon);
  watcher.onDidDelete(refreshSoon);
  watcher.onDidChange(refreshSoon);

  context.subscriptions.push(
    watcher,
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.languageId === "kif" && event.contentChanges.length > 0) refreshSoon();
    }),
    vscode.commands.registerCommand("kifedit.refreshTrees", refreshAll),
  );
}
