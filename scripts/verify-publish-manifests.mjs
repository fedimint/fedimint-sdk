import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const rootDir = process.cwd()
const packagesDir = join(rootDir, 'packages')
const dependencyFields = [
  'dependencies',
  'optionalDependencies',
  'peerDependencies',
]

const packageDirs = readdirSync(packagesDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => join(packagesDir, entry.name))

const failures = []

for (const packageDir of packageDirs) {
  const packageJsonPath = join(packageDir, 'package.json')
  const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf8'))

  if (pkg.private) {
    continue
  }

  const workspaceProtocolEntries = dependencyFields.flatMap((field) => {
    const dependencies = pkg[field] ?? {}
    return Object.entries(dependencies)
      .filter(
        ([, version]) =>
          typeof version === 'string' && version.startsWith('workspace:'),
      )
      .map(([name, version]) => `${field}.${name}=${version}`)
  })

  if (workspaceProtocolEntries.length > 0) {
    failures.push({
      name: pkg.name,
      entries: workspaceProtocolEntries,
    })
  }
}

if (failures.length > 0) {
  console.error(
    [
      'Refusing to publish: one or more publishable package manifests still contain workspace protocol references.',
      'This check must run after `changeset version` has rewritten internal dependency ranges.',
      '',
      ...failures.flatMap(({ name, entries }) => [
        `${name}:`,
        ...entries.map((entry) => `- ${entry}`),
        '',
      ]),
    ]
      .join('\n')
      .trim(),
  )
  process.exit(1)
}

console.log('Verified all publishable package manifests are publish-safe.')
