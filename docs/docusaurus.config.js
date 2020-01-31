module.exports = {
  title: 'Dev-Loop',
  tagline: 'A localized task runner',
  url: 'https://dev-loop.kungfury.dev',
  baseUrl: '/',
  favicon: 'img/favicon.ico',
  organizationName: 'SecurityInsanity',
	projectName: 'dev-loop',
  themeConfig: {
		disableDarkMode: true,
    navbar: {
      title: 'Dev-Loop',
      logo: {
        alt: 'Dev-Loop logo',
        src: 'img/dl-logo.svg',
      },
      links: [
        {to: 'docs/introduction/getting-started', label: 'Docs', position: 'left'},
        {
          href: 'https://github.com/SecurityInsanity/dev-loop',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
  },
  presets: [
    [
      '@docusaurus/preset-classic',
      {
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl:
            'https://github.com/SecurityInsanity/dev-loop/edit/master/docs/',
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],
};
