import { StreamLanguage, type StringStream } from '@codemirror/language'

// 前端只做詞法層級的標色，記號種類與後端詞法器一致（§4.1）
const LOGIC = new Set(['and', 'or', 'not', '=>', '<=>', 'forall', 'exists'])
const DELIMITER = /[\s()";]/

type State = { inString: boolean }

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
  const ch = stream.next() ?? ''
  if (ch === ';') {
    stream.skipToEnd()
    return 'comment'
  }
  if (ch === '"') {
    state.inString = true
    return readString(stream, state)
  }
  if (ch === '(' || ch === ')') return 'paren'
  let word = ch
  while (!stream.eol() && !DELIMITER.test(stream.peek() ?? '')) word += stream.next()
  if (word.startsWith('?') || word.startsWith('@')) return 'variableName'
  if (/^[+-]?\d/.test(word) && !Number.isNaN(Number(word))) return 'number'
  if (LOGIC.has(word)) return 'keyword'
  return null
}

export const kif = StreamLanguage.define<State>({
  name: 'kif',
  startState: () => ({ inString: false }),
  copyState: (state) => ({ ...state }),
  token,
  languageData: { commentTokens: { line: ';' } },
})
