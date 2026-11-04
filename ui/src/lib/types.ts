export type TreeNodeData = {
  id: string
  label: string
  kind: 'class' | 'member' | 'file' | 'group' | 'formula'
  hasChildren: boolean
  otherParents: number
  term: string | null
  location: Location | null
}

export type Location = {
  file: number
  line: number
}

export type Severity = 'error' | 'warning' | 'info'

export type Diagnostic = {
  from: number
  to: number
  line: number
  column: number
  severity: Severity
  code: string
  message: string
  related: Location[]
}

export type Problem = {
  fileId: number
  file: string
  line: number
  severity: Severity
  code: string
  message: string
}

export type FileSummary = {
  id: number
  path: string
  role: 'library' | 'editable'
  formulas: number
  syntaxErrors: number
}

export type DiagnosticsChanged = {
  file: number
  version: number
  diagnostics: Diagnostic[]
}

export type FileContent = {
  id: number
  path: string
  readOnly: boolean
  text: string
  version: number
  savedVersion: number
  errors: Diagnostic[]
}

export type EditOutcome =
  | { status: 'applied'; version: number; errors: Diagnostic[] }
  | { status: 'conflict'; version: number; text: string }

export type ArgView = {
  kind: 'symbol' | 'string' | 'number' | 'other'
  text: string
}

export type FormulaRef = {
  fileId: number
  index: number
  file: string
  line: number
  text: string
  editable: boolean
  args: ArgView[]
}

export type ValueInput = { kind: 'symbol' | 'string' | 'number'; text: string }
type FormulaInput = { file: number; index: number }

export type FormInput =
  | { type: 'addFact'; file: number; relation: string; args: ValueInput[] }
  | { type: 'changeArgument'; formula: FormulaInput; argument: number; value: ValueInput }
  | { type: 'deleteFact'; formula: FormulaInput }
  | { type: 'changeString'; formula: FormulaInput; argument: number; text: string }

export type Preview = {
  line: number
  removed: string[]
  added: string[]
}

export type EditableFile = { id: number; path: string }

export type DocEntry = {
  relation: 'documentation' | 'termFormat'
  language: string
  text: string
  formula: FormulaRef
}

export type Group = {
  items: FormulaRef[]
  total: number
}

export type TermPageData = {
  name: string
  exists: boolean
  kinds: string[]
  documentation: DocEntry[]
  hierarchy: 'subclass' | 'subrelation' | 'subAttribute' | null
  parents: string[]
  ancestors: string[]
  children: string[]
  instances: string[]
  types: string[]
  signature: FormulaRef[]
  arity: number | null
  examples: FormulaRef[]
  declarations: Group
  asArgument: Group
  inRules: Group
  problems: Problem[]
}

export type NameHit = { name: string; kinds: string[] }

export type TextHit = { term: string | null; text: string; formula: FormulaRef }

export type QueryOutcome =
  | { status: 'answers'; columns: string[]; rows: { values: string[]; support: FormulaRef[] }[] }
  | { status: 'needsProver'; message: string }

export type FileState = { version: number; text: string; errors: Diagnostic[] }

export type FormatPlan = {
  changes: { file: number; path: string; preview: Preview }[]
  skipped: string[]
}
