import type { EngineImageSpec } from '@/services/engineImages';

export type EngineVersionOption = {
  label: string;
  value: string;
  image?: string;
  ready?: boolean;
};

/** Strip registry host / library prefix for loose image equality. */
export function normalizeImageRef(image: string): string {
  let s = (image || '').trim();
  if (!s) return '';
  // docker.io / docker.1ms.run / localhost:5000 / registry.example.com:5000/...
  const slash = s.indexOf('/');
  if (slash > 0) {
    const first = s.slice(0, slash);
    if (
      first === 'localhost' ||
      first.includes('.') ||
      first.includes(':')
    ) {
      s = s.slice(slash + 1);
    }
  }
  if (s.startsWith('library/')) s = s.slice('library/'.length);
  return s;
}

export function parseImageTag(image: string): string {
  const lastSlash = image.lastIndexOf('/');
  const name = lastSlash >= 0 ? image.slice(lastSlash + 1) : image;
  if (name.includes('@')) {
    return name.split('@')[0]?.split(':').pop() || 'latest';
  }
  const idx = name.lastIndexOf(':');
  return idx >= 0 ? name.slice(idx + 1) : 'latest';
}

export function imageRepoWithoutTag(image: string): string {
  const norm = normalizeImageRef(image);
  const at = norm.indexOf('@');
  const base = at >= 0 ? norm.slice(0, at) : norm;
  const lastSlash = base.lastIndexOf('/');
  const namePart = lastSlash >= 0 ? base.slice(lastSlash + 1) : base;
  const colon = namePart.lastIndexOf(':');
  if (colon >= 0) {
    return base.slice(0, base.length - (namePart.length - colon));
  }
  return base;
}

export function imageMatches(a: string, b: string): boolean {
  const ra = imageRepoWithoutTag(a);
  const rb = imageRepoWithoutTag(b);
  if (!ra || !rb) return false;
  if (parseImageTag(a) !== parseImageTag(b)) return false;
  if (ra === rb) return true;
  if (ra.endsWith(`/${rb}`) || rb.endsWith(`/${ra}`)) return true;
  const sa = imageShortName(ra);
  const sb = imageShortName(rb);
  return Boolean(sa) && sa === sb;
}

function imageShortName(imageOrRepo: string): string {
  const repo = imageRepoWithoutTag(imageOrRepo);
  const parts = repo.split('/');
  return (parts[parts.length - 1] || '').toLowerCase();
}

/**
 * Local image belongs to engine if:
 * - repo matches catalog image, or same short name (maas/vllm-openai), or
 * - repo matches 镜像源 mapping for this engine (e.g. vllm-lmcache → vllm/vllm-lmcache).
 */
export function localImageBelongsToEngine(
  image: string,
  engine: string,
  catalog: EngineImageSpec[],
  registryRepos?: Record<string, string>,
): boolean {
  const engineKey = (engine || '').toLowerCase();
  if (!engineKey || !image) return false;
  const repo = imageRepoWithoutTag(image);
  const mapped = (registryRepos?.[engineKey] || '').trim();
  if (mapped) {
    const mappedRepo = imageRepoWithoutTag(mapped.includes(':') ? mapped : `${mapped}:x`);
    if (
      imageMatches(repo, mappedRepo) ||
      normalizeImageRef(repo) === normalizeImageRef(mappedRepo) ||
      imageShortName(repo) === imageShortName(mappedRepo)
    ) {
      return true;
    }
  }
  const catalogForEngine = (catalog || []).filter(
    (item) => (item.engine || '').toLowerCase() === engineKey,
  );
  if (!catalogForEngine.length) return false;
  const catalogRepos = catalogForEngine.map((i) => imageRepoWithoutTag(i.image));
  const catalogNames = new Set(catalogRepos.map((r) => imageShortName(r)).filter(Boolean));
  if (
    catalogRepos.some(
      (r) =>
        imageMatches(repo, r) || normalizeImageRef(repo) === normalizeImageRef(r),
    )
  ) {
    return true;
  }
  return catalogNames.has(imageShortName(repo));
}

function isDockerLibraryRef(image: string): boolean {
  const s = (image || '').trim();
  return (
    s.startsWith('docker.io/library/') || s.startsWith('index.docker.io/library/')
  );
}

/** Pick configured repo (quay.io/ascend/…) over docker.io/library aliases. */
export function preferCanonicalLocalImage(
  images: string[],
  engine: string,
  catalog: EngineImageSpec[],
  registryRepos?: Record<string, string>,
): string {
  const list = (images || []).filter(Boolean);
  if (!list.length) return '';
  if (list.length === 1) {
    const only = list[0];
    const mapped = (registryRepos?.[engine.toLowerCase()] || '').trim();
    if (mapped && isDockerLibraryRef(only)) {
      const tag = parseImageTag(only);
      return mapped.includes(':') ? mapped : `${mapped}:${tag}`;
    }
    return only;
  }
  const engineKey = (engine || '').toLowerCase();
  const mapped = (registryRepos?.[engineKey] || '').trim();
  const catalogImgs = (catalog || [])
    .filter((item) => (item.engine || '').toLowerCase() === engineKey)
    .map((item) => item.image);
  const score = (img: string) => {
    let value = 0;
    if (mapped && (imageMatches(img, `${mapped}:x`) || img.startsWith(`${mapped}:`))) {
      value += 8;
    }
    if (catalogImgs.some((c) => imageMatches(img, c))) value += 4;
    const slash = img.indexOf('/');
    const first = slash > 0 ? img.slice(0, slash) : '';
    if (first.includes('.') && first !== 'docker.io' && first !== 'index.docker.io') {
      value += 2;
    }
    if (!isDockerLibraryRef(img)) value += 1;
    return value + Math.min(img.length, 80) / 100;
  };
  const picked = [...list].sort((a, b) => score(b) - score(a))[0];
  if (mapped && isDockerLibraryRef(picked)) {
    const tag = parseImageTag(picked);
    return mapped.includes(':') ? mapped : `${mapped}:${tag}`;
  }
  return picked;
}

/**
 * Build engine_version options for deploy.
 * Prefer local docker images (value = full image ref). If none local, fall back to catalog
 * versions for pull-on-deploy.
 */
export function buildEngineVersionOptions(params: {
  engine: string;
  catalog: EngineImageSpec[];
  localImages: string[];
  registryRepos?: Record<string, string>;
  /** @deprecated unused; kept for call-site compat */
  readyLabel?: string;
  /** @deprecated unused; kept for call-site compat */
  localOnlyLabel?: string;
}): EngineVersionOption[] {
  const engineKey = (params.engine || '').toLowerCase();
  if (!engineKey) return [];

  const catalogForEngine = params.catalog.filter(
    (item) => (item.engine || '').toLowerCase() === engineKey,
  );
  const localForEngine = (params.localImages || []).filter((img) =>
    localImageBelongsToEngine(
      img,
      engineKey,
      params.catalog,
      params.registryRepos,
    ),
  );

  // 以本地为准：有本机镜像时只列本机（含私有仓库同名镜像）
  // 同短名+tag 的 quay / docker.io/library 别名合并为一条，优先仓库映射全名
  if (localForEngine.length > 0) {
    const groups: string[][] = [];
    for (const img of localForEngine) {
      const group = groups.find((items) => items.some((x) => imageMatches(x, img)));
      if (group) group.push(img);
      else groups.push([img]);
    }
    const byImage = new Map<string, EngineVersionOption>();
    for (const group of groups) {
      const picked = preferCanonicalLocalImage(
        group,
        engineKey,
        params.catalog,
        params.registryRepos,
      );
      const tag = parseImageTag(picked);
      byImage.set(picked, {
        label: tag,
        value: picked,
        image: picked,
        ready: true,
      });
    }
    return Array.from(byImage.values()).sort((a, b) =>
      String(a.value).localeCompare(String(b.value)),
    );
  }

  // 本机无该引擎镜像：回退目录版本（部署时可拉取）
  const byVersion = new Map<string, EngineVersionOption>();
  for (const item of catalogForEngine) {
    const cuda = item.cuda && item.cuda !== 'auto' ? ` (${item.cuda})` : '';
    byVersion.set(item.version, {
      label: `${item.version}${cuda}`,
      value: item.version,
      image: item.image,
      ready: false,
    });
  }

  return Array.from(byVersion.values()).sort((a, b) => {
    if (a.value === 'latest') return -1;
    if (b.value === 'latest') return 1;
    return String(a.value).localeCompare(String(b.value));
  });
}
