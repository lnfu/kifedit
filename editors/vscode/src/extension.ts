import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";
import { registerQuery } from "./query";
import { registerTermPage } from "./termPage";
import { registerTrees } from "./tree";

let client: LanguageClient | undefined;

// 依序使用：kifedit.serverPath 設定、插件內附的 server、開發時專案根目錄的 target/debug
function serverCommand(context: vscode.ExtensionContext): string {
  const configured = vscode.workspace.getConfiguration("kifedit").get<string>("serverPath");
  if (configured) return configured;
  const name = process.platform === "win32" ? "kifedit-lsp.exe" : "kifedit-lsp";
  const bundled = path.join(context.extensionPath, "server", name);
  if (fs.existsSync(bundled)) return bundled;
  return path.join(context.extensionPath, "..", "..", "target", "debug", name);
}

export async function activate(context: vscode.ExtensionContext) {
  const serverOptions: ServerOptions = { command: serverCommand(context) };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "kif" }],
    // 新增、刪除 .kif 檔，或修改 .kifignore、kifedit.toml 時通知 server
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/{*.kif,.kifignore,kifedit.toml}"),
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
