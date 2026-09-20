/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    './src/**/*.{js,jsx,ts,tsx}',
  ],
  theme: {
    typography: require('./typography'),
    extend: {
      colors: {
        gradientTransparent: 'rgba(255, 255, 255, 0.3)', // 示例透明度为0.5的白色渐变
        gray: {
          25: '#FCFCFD',
          50: '#F9FAFB',
          100: '#F3F4F6',
          200: '#E5E7EB',
          300: '#D1D5DB',
          400: '#9CA3AF',
          500: '#6B7280',
          700: '#374151',
          800: '#1F2A37',
          900: '#111928',
        },
        /** 注意这里不要改成hex, 不然会导致 比如 text-primary/60 失效 */
        border: 'hsl(var(--border))',
        /** text：default=正文；primary 保留为品牌色（见下方 primary） */
        default: 'hsl(var(--text-default))',
        muted: 'hsl(var(--text-muted))',
        secondary: 'hsl(var(--text-secondary))',
        disabled: 'hsl(var(--text-disabled))',
        danger: 'hsl(var(--text-danger))',
        /** 状态色 */
        success: 'hsl(var(--status-success))',
        warning: 'hsl(var(--status-warning))',
        error: 'hsl(var(--status-error))',
        info: 'hsl(var(--status-info))',
        /** bg / surface */
        background: 'hsl(var(--background))',
        'background-focused': 'hsl(var(--background-focused))',
        'background-muted': 'hsl(var(--background-muted))',
        card: 'hsl(var(--surface-card))',
        'surface-page': 'hsl(var(--surface-page))',
        'surface-card': 'hsl(var(--surface-card))',
        'surface-elevated': 'hsl(var(--surface-elevated))',
        primary: 'hsl(var(--primary))',
        navy: 'var(--c-navy)',
        accent: 'var(--c-accent)',
      },
      fontFamily: {
        body: 'var(--font-body)',
        mono: 'var(--font-mono)',
      },
      ringColor: {
        DEFAULT: 'hsl(var(--focus-ring))',
        primary: 'hsl(var(--primary))',
      },
      borderRadius: {
        lg: 'var(--r-lg)',
        md: 'var(--r-md)',
        sm: 'var(--r-sm)',
        xl: 'var(--r-xl)',
        pill: 'var(--r-pill)',
      },
      screens: {
        mobile: '100px',
        // => @media (min-width: 100px) { ... }
        tablet: '640px', // 391
        // => @media (min-width: 600px) { ... }
        pc: '769px',
        // => @media (min-width: 769px) { ... }
      },
      boxShadow: {
        xs: '0px 1px 2px 0px rgba(16, 24, 40, 0.05)',
        sm: 'var(--tw-ring-offset-shadow, 0 0 #0000), var(--tw-ring-shadow, 0 0 #0000), var(--tw-shadow)',
        md: '0px 2px 4px -2px rgba(16, 24, 40, 0.06), 0px 4px 8px -2px rgba(16, 24, 40, 0.10)',
        lg: '0px 4px 6px -2px rgba(16, 24, 40, 0.03), 0px 12px 16px -4px rgba(16, 24, 40, 0.08)',
        xl: '0px 8px 8px -4px rgba(16, 24, 40, 0.03), 0px 20px 24px -4px rgba(16, 24, 40, 0.08)',
        '2xl': '0px 24px 48px -12px rgba(16, 24, 40, 0.18)',
        '3xl': '0px 32px 64px -12px rgba(16, 24, 40, 0.14)',
        'input-shadow': '0 0 0 3px rgba(0, 69, 176, 0.12)',
        'img-shadow': '0px 0px 0px.5px #dce0e9, 0px 4px 8px 0px rgba(144, 150, 174, .25)',
        pop: 'rgba(0, 0, 0, 0) 0px 0px 0px 0px, rgba(0, 0, 0, 0) 0px 0px 0px 0px, rgba(0, 0, 0, 0.1) 0px 4px 6px -1px, rgba(0, 0, 0, 0.1) 0px 2px 4px -2px',
        card: 'var(--shadow-card)',
        popover: 'var(--shadow-popover)',
        modal: 'var(--shadow-modal)',
        nav: 'var(--shadow-nav)',
        drop: 'var(--shadow-drop)',
      },
      transitionDuration: {
        micro: '120ms',
        DEFAULT: '200ms',
        complex: '260ms',
      },
      transitionTimingFunction: {
        enter: 'cubic-bezier(0.16, 1, 0.3, 1)',
        exit: 'ease-in',
        spring: 'cubic-bezier(0.34, 1.56, 0.64, 1)',
      },
      width: {
        '3/50': '6%',
        '47/50': '94%',
      },
      backgroundSize: {
        full: '100% 100%',
      },
      borderColor: {
        DEFAULT: 'hsl(var(--border))',
      },
      backgroundColor: {
        'black-06': 'rgba(0, 0, 0, 0.06)',
      },
    },
  },
  plugins: [require('@tailwindcss/typography'), require('@tailwindcss/container-queries')],
  // corePlugins: {
  //   preflight: false,
  // }
};
