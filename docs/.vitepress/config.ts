import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Documentation',
  description: 'Project documentation',
  themeConfig: {
    nav: [
      { text: 'Home', link: '/' },
      { text: 'Guide', link: '/guide/' }
    ],
    sidebar: {
      '/guide/': [
        {
          text: 'Guide',
          items: [
            { text: 'Getting Started', link: '/guide/' }
          ]
        }
      ]
    }
  }
})
