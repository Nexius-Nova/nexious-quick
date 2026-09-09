import { createRouter, createWebHashHistory } from 'vue-router'

// 设置窗口的 3 个独立路由，页面组件按需懒加载，进入各自页面才拉取该页数据
/** 设置窗口挂载后空闲预热三个页面 chunk，避免打包环境首次切换页面时动态加载卡顿。 */
export function prefetchSettingsPages() {
  const loaders = [
    () => import('./settings/DataPage.vue'),
    () => import('./settings/AppearancePage.vue'),
    () => import('./settings/ApplicationPage.vue'),
  ]
  const run = () => {
    for (const load of loaders) void load().catch(() => {})
  }
  if (typeof window.requestIdleCallback === 'function') {
    window.requestIdleCallback(run)
  } else {
    window.setTimeout(run, 150)
  }
}

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/data' },
    {
      path: '/data',
      name: 'data',
      component: () => import('./settings/DataPage.vue'),
    },
    {
      path: '/appearance',
      name: 'appearance',
      component: () => import('./settings/AppearancePage.vue'),
    },
    {
      path: '/application',
      name: 'application',
      component: () => import('./settings/ApplicationPage.vue'),
    },
  ],
})