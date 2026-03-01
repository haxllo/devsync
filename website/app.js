(function () {
  const items = document.querySelectorAll('[data-reveal]');
  if (!items.length) return;

  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          entry.target.classList.add('show');
          observer.unobserve(entry.target);
        }
      });
    },
    { threshold: 0.15 }
  );

  items.forEach((item, index) => {
    item.style.transitionDelay = `${Math.min(index * 45, 240)}ms`;
    observer.observe(item);
  });
})();
