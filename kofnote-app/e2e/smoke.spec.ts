import { expect, test } from '@playwright/test'

test.describe('KOF Note desktop UI (mock mode)', () => {
  test('supports language switching and bilingual toasts', async ({ page }) => {
    await page.goto('/')

    await expect(page.getByRole('heading', { name: 'KOF Note' })).toBeVisible()
    await page.locator('.tab-list button', { hasText: /^Settings$/ }).click()

    await page.getByLabel(/UI Language|介面語言/).selectOption('zh-TW')
    await expect(page.getByRole('button', { name: '儀表板' })).toBeVisible()

    await page.getByRole('button', { name: '刷新' }).click()
    await expect(page.locator('.notice.success').last()).toContainText('資料已刷新')
  })

  test('dashboard force graph routes to records filter', async ({ page }) => {
    await page.goto('/')

    await expect(page.getByText('Interactive Knowledge Graph')).toBeVisible()
    const graphNodeCount = await page.locator('.dashboard-force-svg .graph-node').count()
    expect(graphNodeCount).toBeGreaterThanOrEqual(7)

    await page.locator('.dashboard-force-svg .graph-node.kind-type').first().click()
    await expect(page.locator('.tab-list .tab-btn.active')).toContainText('Records')

    const selectedType = await page.locator('.records-filter-grid select').first().inputValue()
    expect(selectedType).not.toBe('all')
  })

  test('renders records constellation and interactive nodes', async ({ page }) => {
    await page.goto('/')
    await page.locator('.tab-list button', { hasText: /^Records$/ }).click()

    await expect(page.getByText('Knowledge Constellation')).toBeVisible()
    await expect(page.locator('.records-constellation .type-node')).toHaveCount(5)
    const tagCount = await page.locator('.records-constellation .tag-node').count()
    expect(tagCount).toBeGreaterThanOrEqual(4)
  })

  test('renders logs pulse and ai insights', async ({ page }) => {
    await page.goto('/')

    await page.locator('.tab-list button', { hasText: /^Logs$/ }).click()
    await expect(page.getByText('Timeline Pulse')).toBeVisible()
    const pulseCount = await page.locator('.logs-pulse-grid .pulse-col').count()
    expect(pulseCount).toBeGreaterThanOrEqual(1)

    await page.locator('.tab-list button', { hasText: /^AI$/ }).click()
    await page.getByRole('button', { name: 'Run Analysis' }).click()
    await expect(page.getByText('Insight Capsules')).toBeVisible()
    await expect(page.locator('.ai-insight-card')).toHaveCount(3)
  })
})
