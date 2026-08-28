import { NextConfig } from 'next'

module.exports = {
  webpack: (config, { isServer }) => {
    //enable asynchronous WebAssembly
    config.experiments = {
      asyncWebAssembly: true,
      layers: true,
    }

    // fix warnings for async functions in the browser
    if (!isServer) {
      config.output.environment = {
        ...config.output.environment,
        asyncFunction: true,
      }
    }

    return config
  },
} as NextConfig
