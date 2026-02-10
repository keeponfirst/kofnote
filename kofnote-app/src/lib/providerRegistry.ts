import type { DebateProviderConfig, DebateProviderRegistrySettings, DebateProviderType } from '../types'

export interface ProviderAdapter {
  readonly id: string
  readonly type: DebateProviderType
  readonly capabilities: string[]
  isEnabled(): boolean
}

abstract class BaseProviderAdapter implements ProviderAdapter {
  readonly id: string
  readonly type: DebateProviderType
  readonly capabilities: string[]
  protected enabled: boolean

  constructor(config: DebateProviderConfig) {
    this.id = config.id
    this.type = config.type
    this.capabilities = [...config.capabilities]
    this.enabled = config.enabled
  }

  isEnabled(): boolean {
    return this.enabled
  }
}

class ConfigProviderAdapter extends BaseProviderAdapter {}

export const DEFAULT_DEBATE_PROVIDER_CONFIGS: DebateProviderConfig[] = [
  {
    id: 'codex-cli',
    type: 'cli',
    enabled: true,
    capabilities: ['debate', 'cli-execution', 'structured-output'],
  },
  {
    id: 'gemini-cli',
    type: 'cli',
    enabled: true,
    capabilities: ['debate', 'cli-execution', 'structured-output'],
  },
  {
    id: 'claude-cli',
    type: 'cli',
    enabled: true,
    capabilities: ['debate', 'cli-execution', 'structured-output'],
  },
  {
    id: 'chatgpt-web',
    type: 'web',
    enabled: true,
    capabilities: ['debate', 'web-automation', 'structured-output'],
  },
  {
    id: 'gemini-web',
    type: 'web',
    enabled: true,
    capabilities: ['debate', 'web-automation', 'structured-output'],
  },
  {
    id: 'claude-web',
    type: 'web',
    enabled: true,
    capabilities: ['debate', 'web-automation', 'structured-output'],
  },
]

function normalizeProviderType(value: string | undefined): DebateProviderType {
  return value === 'web' ? 'web' : 'cli'
}

function normalizeCapabilities(value: string[] | undefined, fallback: string[]): string[] {
  const incoming = (value ?? [])
    .map((item) => item.trim().toLowerCase())
    .filter((item) => item.length > 0)
  const resolved = incoming.length > 0 ? incoming : fallback
  return Array.from(new Set(resolved))
}

function mergeProviderCatalog(input?: DebateProviderConfig[] | null): DebateProviderConfig[] {
  const merged = new Map<string, DebateProviderConfig>()
  for (const base of DEFAULT_DEBATE_PROVIDER_CONFIGS) {
    merged.set(base.id, { ...base, capabilities: [...base.capabilities] })
  }

  for (const item of input ?? []) {
    const id = String(item.id ?? '').trim().toLowerCase()
    if (!id) {
      continue
    }
    const base = merged.get(id)
    merged.set(id, {
      id,
      type: normalizeProviderType(item.type ?? base?.type),
      enabled: item.enabled ?? base?.enabled ?? true,
      capabilities: normalizeCapabilities(item.capabilities, base?.capabilities ?? ['debate']),
    })
  }

  return [...merged.values()]
}

export function buildProviderRegistrySettings(
  input?: DebateProviderRegistrySettings | null,
): DebateProviderRegistrySettings {
  return {
    providers: mergeProviderCatalog(input?.providers),
  }
}

export class ProviderRegistry {
  private readonly providers = new Map<string, ConfigProviderAdapter>()

  constructor(settings?: DebateProviderRegistrySettings | null) {
    this.load(settings)
  }

  load(settings?: DebateProviderRegistrySettings | null): void {
    this.providers.clear()
    for (const config of mergeProviderCatalog(settings?.providers)) {
      this.providers.set(config.id, new ConfigProviderAdapter(config))
    }
  }

  get(id: string): ProviderAdapter | undefined {
    return this.providers.get(id.trim().toLowerCase())
  }

  isEnabled(id: string): boolean {
    return this.get(id)?.isEnabled() ?? false
  }

  list(args?: { enabledOnly?: boolean; type?: DebateProviderType }): ProviderAdapter[] {
    const enabledOnly = args?.enabledOnly ?? false
    return [...this.providers.values()].filter((provider) => {
      if (enabledOnly && !provider.isEnabled()) {
        return false
      }
      if (args?.type && provider.type !== args.type) {
        return false
      }
      return true
    })
  }
}
