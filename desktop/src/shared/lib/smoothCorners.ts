type SmoothCornerPathOptions = {
  cornerRadius: number;
  cornerSmoothing: number;
  height: number;
  width: number;
};

type CornerPathParams = {
  a: number;
  arcSectionLength: number;
  b: number;
  c: number;
  cornerRadius: number;
  d: number;
  p: number;
};

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function toRadians(degrees: number) {
  return (degrees * Math.PI) / 180;
}

function rounded(value: number) {
  return Number.isFinite(value) ? value.toFixed(4) : "0";
}

function getCornerPathParams({
  cornerRadius,
  cornerSmoothing,
  height,
  width,
}: SmoothCornerPathOptions): CornerPathParams {
  const roundingAndSmoothingBudget = Math.min(width, height) / 2;
  const radius = clamp(cornerRadius, 0, roundingAndSmoothingBudget);

  if (radius === 0) {
    return {
      a: 0,
      arcSectionLength: 0,
      b: 0,
      c: 0,
      cornerRadius: 0,
      d: 0,
      p: 0,
    };
  }

  const maxCornerSmoothing = roundingAndSmoothingBudget / radius - 1;
  const smoothing = clamp(cornerSmoothing, 0, Math.min(1, maxCornerSmoothing));
  const p = Math.min((1 + smoothing) * radius, roundingAndSmoothingBudget);

  // Figma-style continuous corners: split each corner into two cubic easing
  // curves around a circular arc. The formulas follow Figma's squircle model.
  const arcMeasure = 90 * (1 - smoothing);
  const arcSectionLength =
    Math.sin(toRadians(arcMeasure / 2)) * radius * Math.sqrt(2);
  const angleAlpha = (90 - arcMeasure) / 2;
  const p3ToP4Distance = radius * Math.tan(toRadians(angleAlpha / 2));
  const angleBeta = 45 * smoothing;
  const c = p3ToP4Distance * Math.cos(toRadians(angleBeta));
  const d = c * Math.tan(toRadians(angleBeta));
  const b = Math.max(0, (p - arcSectionLength - c - d) / 3);
  const a = 2 * b;

  return {
    a,
    arcSectionLength,
    b,
    c,
    cornerRadius: radius,
    d,
    p,
  };
}

export function getSmoothCornerPath(options: SmoothCornerPathOptions) {
  const width = Math.max(0, options.width);
  const height = Math.max(0, options.height);
  const params = getCornerPathParams({ ...options, width, height });
  const { a, arcSectionLength, b, c, cornerRadius, d, p } = params;

  if (width === 0 || height === 0) {
    return "";
  }

  if (cornerRadius === 0) {
    return `M 0 0 L ${rounded(width)} 0 L ${rounded(width)} ${rounded(height)} L 0 ${rounded(height)} Z`;
  }

  return [
    `M ${rounded(width - p)} 0`,
    `c ${rounded(a)} 0 ${rounded(a + b)} 0 ${rounded(a + b + c)} ${rounded(d)}`,
    `a ${rounded(cornerRadius)} ${rounded(cornerRadius)} 0 0 1 ${rounded(arcSectionLength)} ${rounded(arcSectionLength)}`,
    `c ${rounded(d)} ${rounded(c)} ${rounded(d)} ${rounded(b + c)} ${rounded(d)} ${rounded(a + b + c)}`,
    `L ${rounded(width)} ${rounded(height - p)}`,
    `c 0 ${rounded(a)} 0 ${rounded(a + b)} ${rounded(-d)} ${rounded(a + b + c)}`,
    `a ${rounded(cornerRadius)} ${rounded(cornerRadius)} 0 0 1 ${rounded(-arcSectionLength)} ${rounded(arcSectionLength)}`,
    `c ${rounded(-c)} ${rounded(d)} ${rounded(-(b + c))} ${rounded(d)} ${rounded(-(a + b + c))} ${rounded(d)}`,
    `L ${rounded(p)} ${rounded(height)}`,
    `c ${rounded(-a)} 0 ${rounded(-(a + b))} 0 ${rounded(-(a + b + c))} ${rounded(-d)}`,
    `a ${rounded(cornerRadius)} ${rounded(cornerRadius)} 0 0 1 ${rounded(-arcSectionLength)} ${rounded(-arcSectionLength)}`,
    `c ${rounded(-d)} ${rounded(-c)} ${rounded(-d)} ${rounded(-(b + c))} ${rounded(-d)} ${rounded(-(a + b + c))}`,
    `L 0 ${rounded(p)}`,
    `c 0 ${rounded(-a)} 0 ${rounded(-(a + b))} ${rounded(d)} ${rounded(-(a + b + c))}`,
    `a ${rounded(cornerRadius)} ${rounded(cornerRadius)} 0 0 1 ${rounded(arcSectionLength)} ${rounded(-arcSectionLength)}`,
    `c ${rounded(c)} ${rounded(-d)} ${rounded(b + c)} ${rounded(-d)} ${rounded(a + b + c)} ${rounded(-d)}`,
    "Z",
  ].join(" ");
}
