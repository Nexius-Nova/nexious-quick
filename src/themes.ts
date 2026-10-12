export interface SurfacePalette {
  window: string
  sidebar: string
  content: string
  surface: string
  input: string
  text: string
  secondary: string
  muted: string
  border: string
  accent: string
}

export interface UITheme {
  key: string
  label: string
  description: string
  radius: number
  spacing: number
  font: string
  shadow: string
  light: SurfacePalette
  dark: SurfacePalette
}

const sans = "'Segoe UI Variable Text', 'Segoe UI', 'Microsoft YaHei UI', sans-serif"
const palette = (window: string, sidebar: string, content: string, surface: string, input: string,
  text: string, secondary: string, muted: string, border: string, accent: string): SurfacePalette =>
  ({ window, sidebar, content, surface, input, text, secondary, muted, border, accent })

// The selector keeps a compact swatch; each theme controls the entire workspace.
export const UI_THEMES: UITheme[] = [
  { key: 'classic', label: 'Classic', description: '均衡的工作台主题', radius: 6, spacing: 20, font: sans, shadow: 'none',
    light: palette('#fafafa', '#f3f3f4', '#ffffff', '#ffffff', '#f0f0f2', '#242528', '#565a61', '#747982', '#e2e3e6', '#d9363e'),
    dark: palette('#141617', '#181a1c', '#1d1f21', '#242729', '#2c3033', '#f2f3f5', '#bcc1c7', '#929ba5', '#343a3f', '#f0444b') },
  { key: 'clay', label: 'Clay', description: '锐利红的指挥面板', radius: 3, spacing: 18, font: sans, shadow: 'none',
    light: palette('#f6f3f3', '#ebe6e6', '#faf8f8', '#ffffff', '#eee9e9', '#2d2425', '#665558', '#827477', '#d8ccce', '#c93441'),
    dark: palette('#191516', '#211a1c', '#261f21', '#30272a', '#382e32', '#f4ebed', '#cfbdc2', '#a9929a', '#49383e', '#e94150') },
  { key: 'knot', label: 'Knot', description: '结构化的蓝紫聚焦', radius: 8, spacing: 22, font: sans, shadow: 'none',
    light: palette('#f6f6fb', '#efeff7', '#fafaff', '#ffffff', '#ececf5', '#29283b', '#5d5b74', '#7c7992', '#deddeb', '#625ad6'),
    dark: palette('#18181e', '#1d1d25', '#23232d', '#2b2b37', '#323240', '#eeedf8', '#c1bfd6', '#9895af', '#3f3e50', '#8683f6') },
  { key: 'dash', label: 'Dash', description: '清爽的青色运维模式', radius: 5, spacing: 18, font: sans, shadow: 'none',
    light: palette('#f4f8f9', '#eaf1f3', '#fbfdfd', '#ffffff', '#eaf2f4', '#23363c', '#546c75', '#70868f', '#d7e4e8', '#087e91'),
    dark: palette('#151a1c', '#1a2124', '#20292d', '#263237', '#2d3b42', '#e9f2f5', '#b8cbd2', '#8da5b0', '#384b54', '#23bed2') },
  { key: 'forest', label: 'Forest', description: '平静的编辑绿系', radius: 7, spacing: 22, font: sans, shadow: 'none',
    light: palette('#f6f8f5', '#edf1eb', '#fbfdf9', '#ffffff', '#ecf2e9', '#28372d', '#566d5d', '#75887a', '#dbe5d8', '#237d4b'),
    dark: palette('#171c19', '#1c241f', '#232c26', '#29362e', '#314036', '#e9f2eb', '#bed0c2', '#96ac9d', '#3c4e42', '#3fbb75') },
  { key: 'slate', label: 'Slate', description: '致密石墨控制室', radius: 3, spacing: 16, font: sans, shadow: 'none',
    light: palette('#eff1f3', '#e4e8ed', '#f7f8fa', '#ffffff', '#e8edf2', '#242e3b', '#566578', '#788699', '#ccd5df', '#526d8c'),
    dark: palette('#14181e', '#1a2028', '#202731', '#28313d', '#303c4b', '#eef2f7', '#bcc8d7', '#95a7bd', '#3c4c60', '#819bb9') },
  { key: 'paper', label: 'Paper', description: '暖纸文档阅读优先', radius: 2, spacing: 24, font: sans, shadow: 'none',
    light: palette('#f5f2eb', '#eeeadf', '#fbf9f2', '#fffdf7', '#f0eadf', '#3a332a', '#70614f', '#8b7d69', '#e2d8c8', '#986236'),
    dark: palette('#211e19', '#28241e', '#302b24', '#393228', '#42392e', '#f2eadb', '#d3c3ab', '#af9b80', '#514533', '#cb9569') },
  { key: 'amber', label: 'Amber', description: '高对比计划台', radius: 4, spacing: 20, font: sans, shadow: 'none',
    light: palette('#f8f5ee', '#f0eadb', '#fffdf8', '#ffffff', '#f4eddd', '#362e1e', '#726143', '#8f7c57', '#e5d8bd', '#926600'),
    dark: palette('#1b1914', '#242017', '#2b261b', '#342d1e', '#3f3522', '#fbf1d9', '#dcca9c', '#baa577', '#53442a', '#f3ac23') },
]

export function uiTheme(key: string): UITheme {
  const legacy: Record<string, string> = { blue: 'classic', violet: 'knot', cyan: 'dash', emerald: 'forest', orange: 'paper', rose: 'clay' }
  return UI_THEMES.find((theme) => theme.key === (legacy[key] ?? key)) ?? UI_THEMES[0]
}

export function themeVariables(theme: UITheme, dark: boolean): Record<string, string> {
  const p = dark ? theme.dark : theme.light
  const rgb = (hex: string) => [1, 3, 5].map((index) => parseInt(hex.slice(index, index + 2), 16)).join(', ')
  return {
    '--primary': p.accent, '--primary-strong': p.accent, '--primary-rgb': rgb(p.accent),
    '--on-primary': dark ? p.window : '#ffffff',
    '--text': p.text, '--text2': p.secondary, '--sub': p.muted,
    '--hairline': p.border, '--hairline2': p.border,
    '--hover': `rgba(${rgb(p.accent)}, 0.08)`, '--hover-strong': `rgba(${rgb(p.accent)}, 0.14)`,
    '--seg-bg': p.input, '--icon-plain-bg': p.input, '--input-bg': p.input,
    '--panel-rgb': rgb(p.surface),
    '--window-bg': p.window, '--sider-bg': p.sidebar, '--content-bg': p.content,
    '--surface': p.surface, '--ui-radius': `${theme.radius}px`, '--ui-spacing': `${theme.spacing}px`,
    '--ui-font': theme.font, '--ui-shadow': theme.shadow,
  }
}
