import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { execSync } from 'child_process'

// Get git commit hash at build time
const getGitHash = () => {
  // Check environment variable first (set in Docker build)
  if (process.env.VITE_COMMIT_HASH && process.env.VITE_COMMIT_HASH !== 'unknown') {
    return process.env.VITE_COMMIT_HASH
  }
  // Fall back to git command for local development
  try {
    return execSync('git rev-parse --short HEAD').toString().trim()
  } catch {
    return 'unknown'
  }
}

// Get build time
const getBuildTime = () => {
  if (process.env.VITE_BUILD_TIME && process.env.VITE_BUILD_TIME !== 'unknown') {
    return process.env.VITE_BUILD_TIME
  }
  return new Date().toISOString()
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [tailwindcss(), react()],
  base: '/',
  define: {
    __COMMIT_HASH__: JSON.stringify(getGitHash()),
    __BUILD_TIME__: JSON.stringify(getBuildTime()),
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        ws: true,
      },
      '/auth': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
})
