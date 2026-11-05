import * as path from "node:path";
import * as vscode from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";
import { registerQuery } from "./query";
import { registerTermPage } from "./termPage";
import { registerTrees } from "./tree";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext) {
  const configured = vscode.workspace.getConfiguration("kifedit").get<string>("serverPath");
  // 開發時：editors/vscode → 專案根目錄的 target/debug
  const command =
    configured || path.join(context.extensionPath, "..", "..", "target", "debug", "kifedit-lsp");

  const serverOptions: ServerOptions = { command };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "kif" }],
    // 新增、刪除 .kif 檔，或修改 .kifignore、project.toml 時通知 server
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/{*.kif,.kifignore,project.toml}"),
    },
    // hover 裡的 Open Term Page 連結
    markdown: { isTrusted: { enabledCommands: ["kifedit.openTermPage"] } },
  };

  client = new LanguageClient("kifedit", "kifedit", serverOptions, clientOptions);
  await client.start();
  registerTrees(context, client);
  registerQuery(context, client);
  registerTermPage(context, client);
}

export function deactivate() {
  return client?.stop();
}
