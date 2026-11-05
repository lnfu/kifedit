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

  context.subscriptions.push(
    vscode.commands.registerCommand("kifedit.refreshTrees", () => trees.forEach((t) => t.refresh())),
  );
}
