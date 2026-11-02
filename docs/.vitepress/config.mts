import { defineConfig } from 'vitepress'

const repo = 'https://github.com/ijevim-org/ijevim'

export default defineConfig({
  lang: 'ja-JP',
  title: 'ijevim',
  description: 'Rust 製の軽量 Vim ライク TUI エディタ',
  base: process.env.DOCS_BASE ?? '/',
  cleanUrls: true,
  lastUpdated: true,

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/logo.svg' }],
    ['meta', { name: 'theme-color', content: '#0f1116' }],
  ],

  themeConfig: {
    logo: '/logo.svg',
    siteTitle: 'ijevim',

    nav: [
      { text: 'ガイド', link: '/keybindings' },
      { text: 'プラグイン', link: '/plugins' },
      { text: '開発', link: '/progress' },
    ],

    sidebar: [
      {
        text: 'はじめに',
        items: [
          { text: '概要', link: '/' },
          { text: 'キーバインド仕様', link: '/keybindings' },
        ],
      },
      {
        text: 'キーマップ',
        items: [
          { text: 'Vim 互換レベル', link: '/keymap-vim' },
          { text: 'Emacs 互換レベル', link: '/keymap-emacs' },
          { text: 'Emacs Mark/Region 設計', link: '/design/emacs-mark-region' },
        ],
      },
      {
        text: 'プラグイン',
        items: [{ text: 'プラグインシステム', link: '/plugins' }],
      },
      {
        text: '開発',
        items: [
          { text: 'レビューモードの進捗', link: '/progress' },
          { text: 'リリースチェックリスト', link: '/release-checklist' },
        ],
      },
    ],

    socialLinks: [{ icon: 'github', link: repo }],

    editLink: {
      pattern: `${repo}/edit/main/docs/:path`,
      text: 'GitHub でこのページを編集',
    },

    search: { provider: 'local' },

    outline: { label: '目次', level: [2, 3] },
    docFooter: { prev: '前へ', next: '次へ' },
    lastUpdated: { text: '最終更新' },
    darkModeSwitchLabel: 'テーマ',
    returnToTopLabel: 'トップへ戻る',
    sidebarMenuLabel: 'メニュー',

    footer: {
      message: 'MIT License',
      copyright: '© NazoVim-org / ijevim contributors',
    },
  },
})
