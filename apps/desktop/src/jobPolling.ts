import type { Job, ResultPage } from './types';
import { appendResultPage, attachCatalog } from './results';
import type { Catalog } from './results';

export interface JobTransport {
  status: (id: string) => Promise<Job>;
  results: (id: string, after: number) => Promise<ResultPage>;
}
export interface JobUpdate { job: Job; catalog: Catalog; hasMore: boolean }

/** Publish status first, then bounded pages. A later page failure retains earlier pages. */
export async function pollJob(id: string, scan: boolean, catalog: Catalog, transport: JobTransport, publish: (update: JobUpdate) => void, isCurrent: () => boolean): Promise<JobUpdate | null> {
  const status = await transport.status(id);
  if (!isCurrent()) return null;
  let update: JobUpdate = { job: scan ? attachCatalog(status, catalog) : status, catalog, hasMore: scan };
  publish(update);
  if (!scan) return update;
  for (let batch = 0; batch < 4; batch++) {
    const page = await transport.results(id, update.catalog.cursor);
    if (!isCurrent()) return null;
    const next = appendResultPage(update.catalog, page);
    update = { job: attachCatalog(status, next), catalog: next, hasMore: page.hasMore };
    publish(update);
    if (!page.hasMore) break;
  }
  return update;
}
