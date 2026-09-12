/** Единый поиск по русским и английским именам без различия е/ё. */
export function matchesSearch(query: string, fields: readonly (string | null | undefined)[]): boolean {
  const normalize = (text: string) => text.normalize("NFKC").toLocaleLowerCase("ru").replaceAll("ё", "е").replace(/\s+/g, " ").trim();
  const text = normalize(fields.filter(Boolean).join(" "));
  return normalize(query).split(" ").every(word => text.includes(word));
}
