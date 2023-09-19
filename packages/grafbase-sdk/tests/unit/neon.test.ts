import { config, g, connector } from '../../src/index'
import { describe, expect, it, beforeEach } from '@jest/globals'
import { renderGraphQL } from '../utils'

describe('OpenAPI generator', () => {
  beforeEach(() => g.clear())

  it('generates the minimum possible Neon datasource', () => {
    const postgres = connector.Neon('Postgres', {
      url: 'postgres://postgres:grafbase@localhost:5432/postgres'
    })

    g.datasource(postgres)

    expect(renderGraphQL(config({ schema: g }))).toMatchInlineSnapshot(`
      "extend schema
        @neon(
          name: "Postgres"
          url: "postgres://postgres:grafbase@localhost:5432/postgres"
          namespace: true
        )"
    `)
  })

  it('generates a Neon datasource with negative namespace', () => {
    const postgres = connector.Neon('Postgres', {
      url: 'postgres://postgres:grafbase@localhost:5432/postgres'
    })

    g.datasource(postgres, { namespace: false })

    expect(renderGraphQL(config({ schema: g }))).toMatchInlineSnapshot(`
      "extend schema
        @neon(
          name: "Postgres"
          url: "postgres://postgres:grafbase@localhost:5432/postgres"
          namespace: false
        )"
    `)
  })

  it('generates a Neon datasource with positive namespace', () => {
    const postgres = connector.Neon('Postgres', {
      url: 'postgres://postgres:grafbase@localhost:5432/postgres'
    })

    g.datasource(postgres, { namespace: true })

    expect(renderGraphQL(config({ schema: g }))).toMatchInlineSnapshot(`
      "extend schema
        @neon(
          name: "Postgres"
          url: "postgres://postgres:grafbase@localhost:5432/postgres"
          namespace: true
        )"
    `)
  })
})
