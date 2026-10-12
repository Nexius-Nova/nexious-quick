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

const sans = "'Segoe UI', 'Microsoft YaHei UI', sans-serif"
const serif = "'Georgia', 'Noto Serif SC', 'Microsoft YaHei', serif"
const palette = (window: string, sidebar: string, content: string, surface: string, input: string,
  text: string, secondary: string, muted: string, border: string, accent: string): SurfacePalette =>
  ({ window, sidebar, content, surface, input, text, secondary, muted, border, accent })

// Keep the old accent keys so upgrading preserves the user's chosen family.
export const UI_THEMES: UITheme[] = [
  { key: 'blue', label: '澄净', description: '明亮留白 · 清晰层次', radius: 10, spacing: 24, font: sans,
    shadow: '0 4px 20px rgb(27 47 81 / 4%)',
    light: palette('#eaf0f8', '#e4ebf5', '#f4f7fb', '#ffffff', '#edf2f9', '#1e2e47', '#445771', '#667b96', '#d4deec', '#3264bb'),
    dark: palette('#101823', '#152032', '#192537', '#202e43', '#152131', '#e6edf7', '#b7c8df', '#899fbd', '#33455f', '#8eb8ff') },
  { key: 'violet', label: '柔雾', description: '柔和面板 · 舒展圆角', radius: 18, spacing: 28, font: sans,
    shadow: '0 8px 28px rgb(74 45 100 / 6%)',
    light: palette('#eee9f3', '#e9e0f0', '#f5f1f8', '#fffcff', '#eee7f3', '#3b2c49', '#655471', '#85718f', '#ded1e7', '#8152a3'),
    dark: palette('#1d1725', '#251c30', '#2b2236', '#342a40', '#231b2d', '#f0e5f6', '#ceb9dd', '#a78db8', '#50405e', '#cca4ec') },
  { key: 'cyan', label: '冰川', description: '冷调玻璃 · 轻盈边界', radius: 12, spacing: 24, font: sans,
    shadow: '0 6px 22px rgb(24 79 91 / 5%)',
    light: palette('#dfedf0', '#dcecf0', '#edf6f7', '#f8ffff', '#e2f0f3', '#213e46', '#4d6b74', '#65818b', '#c4dfe4', '#24798c'),
    dark: palette('#102328', '#142e34', '#193238', '#203e45', '#122b32', '#e1f3f5', '#b2d2d8', '#87aeb6', '#34545e', '#80cede') },
  { key: 'emerald', label: '林间', description: '苔绿底色 · 自然质感', radius: 9, spacing: 26, font: sans,
    shadow: '0 4px 18px rgb(44 63 42 / 4%)',
    light: palette('#e7ecdf', '#dfe7d8', '#f1f4eb', '#fbfcf5', '#e8eee1', '#303e2d', '#56664e', '#75806b', '#d3dcc9', '#567243'),
    dark: palette('#192218', '#202d1f', '#263124', '#303d2c', '#1f2a1e', '#e6eede', '#c2d1b6', '#97ac87', '#46543e', '#b1ce8f') },
  { key: 'orange', label: '纸页', description: '暖纸底色 · 书刊排版', radius: 3, spacing: 28, font: serif,
    shadow: 'none',
    light: palette('#e8dfcf', '#e9dfcb', '#f5efdf', '#fff9eb', '#eee5d3', '#42372b', '#6d5d49', '#8b7760', '#d8c9af', '#93603c'),
    dark: palette('#241e17', '#30271d', '#362d22', '#40362a', '#2a231a', '#f2e6d1', '#d7c6a9', '#af997c', '#5b4c38', '#d8ad7c') },
  { key: 'rose', label: '柔砂', description: '低饱和暖灰 · 柔软卡片', radius: 16, spacing: 26, font: sans,
    shadow: '0 5px 22px rgb(89 51 55 / 5%)',
    light: palette('#efdfda', '#ead7d2', '#faf0eb', '#fff9f5', '#f1e2dc', '#4d3535', '#7a5b58', '#987773', '#e5cec5', '#a05d60'),
    dark: palette('#2c1d1e', '#362426', '#3c2b2b', '#493334', '#302123', '#f8e5df', '#dcbcb3', '#b58e87', '#624448', '#e6a7aa') },
  { key: 'slate', label: '石墨', description: '中性对比 · 紧凑工作台', radius: 4, spacing: 18, font: sans,
    shadow: 'none',
    light: palette('#e0e1e2', '#dedfe0', '#efeff0', '#fafafa', '#e6e7e8', '#242629', '#50545a', '#70757c', '#c9cdd1', '#4c596b'),
    dark: palette('#111213', '#191b1e', '#202226', '#292c31', '#17191c', '#eceef1', '#c1c6ce', '#949ca7', '#42474f', '#b6c5d9') },
]

export function uiTheme(key: string): UITheme {
  return UI_THEMES.find((theme) => theme.key === key) ?? UI_THEMES[0]
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
