import { HighlightStyle, StreamLanguage, StringStream, syntaxHighlighting } from '@codemirror/language'
import { tags } from '@lezer/highlight'

// 前端只做詞法層級的標色，記號種類與後端詞法器一致（§4.1）
const LOGIC = new Set(['and', 'or', 'not', '=>', '<=>', 'forall', 'exists'])
const DELIMITER = /[\s()";]/

type State = { inString: boolean; afterParen: boolean }

// 字串可跨行：未結束時記在 state 中，下一行繼續
function readString(stream: StringStream, state: State) {
  while (!stream.eol()) {
    const ch = stream.next()
    if (ch === '\\') stream.next()
    else if (ch === '"') {
      state.inString = false
      break
    }
  }
  return 'string'
}

function token(stream: StringStream, state: State): string | null {
  if (state.inString) return readString(stream, state)
  if (stream.eatSpace()) return null
  const afterParen = state.afterParen
  state.afterParen = false
  const ch = stream.next() ?? ''
  if (ch === ';') {
    stream.skipToEnd()
    return 'comment'
  }
  if (ch === '"') {
    state.inString = true
    return readString(stream, state)
  }
  if (ch === '(') {
    state.afterParen = true
    return 'paren'
  }
  if (ch === ')') return 'paren'
  let word = ch
  while (!stream.eol() && !DELIMITER.test(stream.peek() ?? '')) word += stream.next()
  if (word.startsWith('?') || word.startsWith('@')) return 'variableName'
  if (/^[+-]?\d/.test(word) && !Number.isNaN(Number(word))) return 'number'
  if (LOGIC.has(word)) return 'keyword'
  // 公式開頭的符號是關係名
  if (afterParen) return 'variableName.function'
  return null
}

const startState = (): State => ({ inString: false, afterParen: false })

export const kif = StreamLanguage.define<State>({
  name: 'kif',
  startState,
  copyState: (state) => ({ ...state }),
  token,
  languageData: { commentTokens: { line: ';' } },
})

// 顏色取自 app.css 的設計代號，切換主題時自動跟著改
export const kifHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.function(tags.variableName), color: 'var(--syn-relation)' },
    { tag: tags.variableName, color: 'var(--syn-variable)' },
    { tag: tags.string, color: 'var(--syn-string)' },
    { tag: tags.comment, color: 'var(--syn-comment)', fontStyle: 'italic' },
    { tag: tags.keyword, color: 'var(--syn-keyword)' },
    { tag: tags.number, color: 'var(--syn-variable)' },
  ]),
)

const CLASSES: Record<string, string> = {
  'variableName.function': 'syn-relation',
  variableName: 'syn-variable',
  string: 'syn-string',
  comment: 'syn-comment',
  keyword: 'syn-keyword',
  number: 'syn-variable',
}

export type Span = { text: string; cls: string | null }

// 詞彙頁等唯讀處的公式標色，與編輯器使用同一個詞法器
export function highlight(text: string): Span[] {
  const state = startState()
  const spans: Span[] = []
  for (const [i, line] of text.split('\n').entries()) {
    if (i > 0) spans.push({ text: '\n', cls: null })
    const stream = new StringStream(line, 4, 2)
    while (!stream.eol()) {
      const type = token(stream, state)
      spans.push({ text: stream.current(), cls: type ? (CLASSES[type] ?? null) : null })
      stream.start = stream.pos
    }
  }
  return spans
}
