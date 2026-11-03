export type TreeNodeData = {
  name: string
  kind: 'class' | 'instance'
  hasChildren: boolean
  otherParents: number
}
