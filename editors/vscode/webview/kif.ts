// CodeMirror StringStream 的最小版本：只實作詞法器用到的方法
class StringStream {
  pos = 0
  start = 0
  constructor(private string: string) {}
  eol() {
    return this.pos >= this.string.length
  }
  peek(): string | undefined {
    return this.string.charAt(this.pos) || undefined
  }
  next(): string | undefined {
    if (this.pos < this.string.length) return this.string.charAt(this.pos++)
    return undefined
  }
  eatSpace() {
    const start = this.pos
    while (/\s/.test(this.string.charAt(this.pos))) this.pos++
    return this.pos > start
  }
  skipToEnd() {
    this.pos = this.string.length
  }
  current() {
    return this.string.slice(this.start, this.pos)
  }
}

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
    const stream = new StringStream(line)
    while (!stream.eol()) {
      const type = token(stream, state)
      spans.push({ text: stream.current(), cls: type ? (CLASSES[type] ?? null) : null })
      stream.start = stream.pos
    }
  }
  return spans
}
