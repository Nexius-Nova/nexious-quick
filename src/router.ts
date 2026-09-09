import { createRouter, createWebHashHistory } from 'vue-router'

// 设置窗口的 3 个独立路由，页面组件按需懒加载，进入各自页面才拉取该页数据
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