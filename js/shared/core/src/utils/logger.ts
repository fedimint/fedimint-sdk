import type { TransportLogger } from '@fedimint/types'

const logLevels = ['debug', 'info', 'warn', 'error', 'none'] as const
export type LogLevel = (typeof logLevels)[number]

export class Logger {
  private level: LogLevel
  private logger: TransportLogger

  /**
   * Generic Logger for a given environment.
   *
   * @param _logger - The transport's logger to use. (console for web, react native, etc.)
   * @param level - The log level to use. (debug, info, warn, error, none)
   */
  constructor(_logger: TransportLogger = console, level: LogLevel = 'none') {
    this.logger = _logger
    this.level = level
  }

  setLevel(level: LogLevel) {
    this.level = level
  }

  private coerceLevel(level: string): LogLevel {
    if (logLevels.includes(level.toLowerCase() as LogLevel)) {
      return level.toLowerCase() as LogLevel
    }
    return 'info'
  }

  log(level: string, message: string, ...args: any[]) {
    const logLevel = this.coerceLevel(level)
    if (!this.shouldLog(logLevel)) {
      return
    }
    const consoleFn = this.logger[logLevel]
    consoleFn(`[${logLevel.toUpperCase()}] ${message}`, ...args)
  }

  debug(message: string, ...args: any[]) {
    this.log('debug', message, ...args)
  }

  info(message: string, ...args: any[]) {
    this.log('info', message, ...args)
  }

  warn(message: string, ...args: any[]) {
    this.log('warn', message, ...args)
  }

  error(message: string, ...args: any[]) {
    this.log('error', message, ...args)
  }

  private shouldLog(
    messageLevel: LogLevel,
  ): messageLevel is Exclude<LogLevel, 'none'> {
    const levels: LogLevel[] = ['debug', 'info', 'warn', 'error', 'none']
    const messageLevelIndex = levels.indexOf(messageLevel)
    const currentLevelIndex = levels.indexOf(this.level)
    return (
      currentLevelIndex <= messageLevelIndex &&
      this.level !== 'none' &&
      messageLevel !== 'none'
    )
  }
}
