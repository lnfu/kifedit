export type Severity = 'error' | 'warning' | 'info'

export type Problem = {
  fileId: number
  file: string
  line: number
  severity: Severity
  code: string
  message: string
}

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
