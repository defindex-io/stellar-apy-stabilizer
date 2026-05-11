export function log(message: string): void {
  const ts = new Date().toISOString();
  console.log(`[${ts}] ${message}`);
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
