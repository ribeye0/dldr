import type { Product, ProductDetail } from "$lib/model/types";
import { creditTextForKind, productType } from "$lib/utils/products";

// Keep the cached gallery responsive and cap anonymous metadata lookups across pages.
export class CreditHydrationController {
  private queue: string[] = [];
  private active = new Set<string>();
  private attempted = new Set<string>();
  private disposed = false;

  constructor(
    private port: { getProductDetail(workId: string): Promise<ProductDetail> },
    private view: { updated(detail: ProductDetail): void; pending(ids: string[]): void },
  ) {}

  show(products: Product[]) {
    if (this.disposed) return;
    this.queue = [...new Set(products.filter((product) =>
      productType(product).tone === "audio" && !creditTextForKind(product, "voice"),
    ).map((product) => product.workId))].filter((id) => !this.attempted.has(id) && !this.active.has(id));
    this.publish();
    this.pump();
  }

  clearQueue() { this.queue = []; this.publish(); }
  retry() { this.attempted.clear(); this.clearQueue(); }
  dispose() { this.disposed = true; this.queue = []; this.view.pending([]); }

  private publish() {
    if (!this.disposed) this.view.pending([...this.active, ...this.queue]);
  }

  private pump() {
    while (!this.disposed && this.active.size < 2 && this.queue.length) {
      const id = this.queue.shift()!;
      this.active.add(id);
      this.attempted.add(id);
      this.publish();
      void this.hydrate(id);
    }
  }

  private async hydrate(id: string) {
    try {
      const detail = await this.port.getProductDetail(id);
      if (!this.disposed) this.view.updated(detail);
    } catch {
      // Preserve the cached card offline; explicit Reload permits a retry.
    } finally {
      this.active.delete(id);
      this.publish();
      this.pump();
    }
  }
}
