(() => {
  const scene = document.querySelector('.run');
  const route = scene.querySelector('#route');
  const body = scene.querySelector('.body');
  const head = scene.querySelector('.head');
  const food = scene.querySelector('.food');
  const still = matchMedia('(prefers-reduced-motion: reduce)');
  const length = route.getTotalLength();
  const meal = 158;
  const mouthReach = 8; // Front edge of the head, not the tongue tip.
  const pelletRadius = food.r.baseVal.value;
  const bite = meal - mouthReach;
  const spacing = 7;
  const start = 96;
  const original = body.children.length;
  const point = d => route.getPointAtLength(d);
  const target = point(meal);
  food.setAttribute('cx', target.x);
  food.setAttribute('cy', target.y);

  function draw(distance) {
    const growth = Math.min(3, Math.max(0, distance - bite) / spacing);
    const count = original + Math.ceil(growth);
    const tail = distance - (original + growth) * spacing;
    while (body.children.length < count) body.append(body.firstElementChild.cloneNode());
    while (body.children.length > count) body.lastElementChild.remove();
    Array.from(body.children).forEach((joint, tailIndex) => {
      // A new joint separates from the held tail as the head moves ahead.
      const d = Math.max(tail, distance - (count - tailIndex) * spacing);
      const p = point(d);
      joint.setAttribute('cx', p.x);
      joint.setAttribute('cy', p.y);
      joint.setAttribute('r', Math.min(4.6, 2.8 + (d - tail) / spacing * .48));
    });
    const p = point(distance), prev = point(distance - 1);
    const angle = Math.atan2(p.y - prev.y, p.x - prev.x) * 180 / Math.PI;
    head.setAttribute('transform', `translate(${p.x} ${p.y}) rotate(${angle})`);
    food.style.opacity = Math.max(0, Math.min(1, (bite - distance) / pelletRadius));
  }

  let frame, elapsed = 0, previous, visible = false;
  const duration = 4200;
  const braking = 600;
  function tick(now) {
    if (previous !== undefined) elapsed = Math.min(duration, elapsed + now - previous);
    previous = now;
    // Cruise, then decelerate uniformly to zero over the final stretch.
    const slowing = Math.max(0, elapsed - (duration - braking));
    const progress = (elapsed - slowing * slowing / (2 * braking)) / (duration - braking / 2);
    draw(start + (length - start) * progress);
    if (elapsed < duration) frame = requestAnimationFrame(tick);
  }
  function sync() {
    cancelAnimationFrame(frame);
    previous = undefined;
    if (still.matches) { elapsed = 0; draw(start); }
    else if (visible && !document.hidden && elapsed < duration) frame = requestAnimationFrame(tick);
  }
  draw(start);
  new IntersectionObserver(([entry]) => { visible = entry.isIntersecting; sync(); }).observe(scene);
  still.addEventListener('change', sync);
  document.addEventListener('visibilitychange', sync);
})();
