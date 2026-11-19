/** Only the latest request may publish state, including errors and loading flags. */
export class RequestGeneration {
  private generation = 0;
  invalidate() { return ++this.generation; }
  current(generation: number) { return generation === this.generation; }
}
