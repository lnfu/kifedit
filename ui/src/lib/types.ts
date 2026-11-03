export type TreeNodeData = {
  id: string
  label: string
  kind: 'class' | 'member' | 'file' | 'group' | 'formula'
  hasChildren: boolean
  otherParents: number
  term: string | null
}

export type FormulaRef = {
  file: string
  line: number
  text: string
}

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
}
