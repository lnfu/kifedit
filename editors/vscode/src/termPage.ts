import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

const WORD = /[^\s()";]+/;

export function registerTermPage(context: vscode.ExtensionContext, client: LanguageClient) {
  const root = vscode.Uri.joinPath(context.extensionUri, "out", "webview");
  let panel: vscode.WebviewPanel | undefined;

  function show(name: string) {
    if (panel) {
      panel.reveal();
      panel.webview.postMessage({ type: "show", name });
      return;
    }
    panel = vscode.window.createWebviewPanel("kifedit.termPage", name, vscode.ViewColumn.Beside, {
      enableScripts: true,
      localResourceRoots: [root],
      retainContextWhenHidden: true,
    });
    const webview = panel.webview;
    webview.html = html(webview, root, name);
    webview.onDidReceiveMessage(async (message) => {
      switch (message.type) {
        case "request":
          try {
            const result = await client.sendRequest(message.method, message.params);
            webview.postMessage({ type: "response", id: message.id, result });
          } catch (e) {
            webview.postMessage({ type: "response", id: message.id, error: e instanceof Error ? e.message : String(e) });
          }
          break;
        case "open": {
          const line = Math.max(message.line - 1, 0);
          await vscode.window.showTextDocument(vscode.Uri.file(message.file), {
            viewColumn: vscode.ViewColumn.One,
            selection: new vscode.Range(line, 0, line, 0),
            preview: true,
          });
          break;
        }
        case "title":
          if (panel) panel.title = message.name;
          break;
      }
    });
    panel.onDidDispose(() => (panel = undefined));
  }

  // 檔案修改後重新載入頁面；連續輸入時只載入一次
  let timer: NodeJS.Timeout | undefined;
  context.subscriptions.push(
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (!panel || event.document.languageId !== "kif") return;
      clearTimeout(timer);
      timer = setTimeout(() => panel?.webview.postMessage({ type: "changed" }), 300);
    }),
    // 從選單執行時，VS Code 傳入的第一個參數不是名稱
    vscode.commands.registerCommand("kifedit.openTermPage", async (arg?: unknown) => {
      const name =
        (typeof arg === "string" ? arg : undefined) ??
        wordAtCursor() ??
        (await vscode.window.showInputBox({ prompt: "Term name" }));
      if (name) show(name);
    }),
  );
}

function wordAtCursor(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return undefined;
  const range = editor.document.getWordRangeAtPosition(editor.selection.active, WORD);
  return range ? editor.document.getText(range) : undefined;
}

function html(webview: vscode.Webview, root: vscode.Uri, name: string): string {
  const nonce = Array.from({ length: 32 }, () => Math.floor(Math.random() * 36).toString(36)).join("");
  const script = webview.asWebviewUri(vscode.Uri.joinPath(root, "index.js"));
  const style = webview.asWebviewUri(vscode.Uri.joinPath(root, "index.css"));
  const escaped = name.replace(/[&"<>]/g, (c) => `&#${c.charCodeAt(0)};`);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; font-src ${webview.cspSource}; script-src 'nonce-${nonce}';" />
  <link rel="stylesheet" href="${style}" />
</head>
<body data-name="${escaped}">
  <script nonce="${nonce}" src="${script}"></script>
</body>
</html>`;
}
