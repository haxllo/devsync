(function () {
  var year = document.getElementById("year");
  if (year) {
    year.textContent = String(new Date().getFullYear());
  }

  var analyticsEndpoint = window.DEVSYNC_ANALYTICS_ENDPOINT || "";
  var sessionId = loadSessionId();

  function loadSessionId() {
    var key = "devsync_session_id";
    try {
      var existing = sessionStorage.getItem(key);
      if (existing) {
        return existing;
      }
      var generated = "s_" + Math.random().toString(36).slice(2) + Date.now().toString(36);
      sessionStorage.setItem(key, generated);
      return generated;
    } catch (error) {
      return "s_anonymous";
    }
  }

  function track(eventName, props) {
    var payload = {
      event: eventName,
      ts: new Date().toISOString(),
      path: window.location.pathname,
      session_id: sessionId,
      props: props || {},
    };

    window.dataLayer = window.dataLayer || [];
    window.dataLayer.push(payload);

    if (!analyticsEndpoint) {
      return;
    }

    try {
      var body = JSON.stringify(payload);
      if (navigator.sendBeacon) {
        navigator.sendBeacon(analyticsEndpoint, body);
        return;
      }
      fetch(analyticsEndpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: body,
        keepalive: true,
      });
    } catch (error) {
      // Intentionally ignore tracking transport failures.
    }
  }

  function bindClickTracking() {
    var trackedLinks = document.querySelectorAll("[data-track]");
    trackedLinks.forEach(function (node) {
      node.addEventListener("click", function () {
        track("cta_click", {
          id: node.getAttribute("data-track") || "unknown",
          label: (node.textContent || "").trim(),
          href: node.getAttribute("href") || "",
        });
      });
    });
  }

  function bindScrollDepthTracking() {
    var firedMarks = {};
    var marks = [25, 50, 75, 100];

    function onScroll() {
      var maxScroll = document.documentElement.scrollHeight - window.innerHeight;
      if (maxScroll <= 0) {
        return;
      }
      var percentage = Math.round((window.scrollY / maxScroll) * 100);
      marks.forEach(function (mark) {
        if (percentage >= mark && !firedMarks[mark]) {
          firedMarks[mark] = true;
          track("scroll_depth", { percent: mark });
        }
      });
    }

    window.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
  }

  function bindLeadForm() {
    var form = document.getElementById("lead-form");
    if (!form) {
      return;
    }

    var status = document.getElementById("lead-status");

    form.addEventListener("submit", function (event) {
      event.preventDefault();

      var formData = new FormData(form);
      var lead = {
        name: String(formData.get("name") || "").trim(),
        email: String(formData.get("email") || "").trim(),
        company: String(formData.get("company") || "").trim(),
        team_size: String(formData.get("team_size") || "").trim(),
        repo_url: String(formData.get("repo_url") || "").trim(),
        message: String(formData.get("message") || "").trim(),
      };

      if (!lead.name || !lead.email || !lead.team_size) {
        if (status) {
          status.textContent = "Fill name, work email, and team size.";
        }
        track("lead_submit_invalid");
        return;
      }

      track("lead_submit", {
        team_size: lead.team_size,
        has_company: Boolean(lead.company),
        has_repo_url: Boolean(lead.repo_url),
      });

      var subject = encodeURIComponent(
        "DevSync Pilot Request - " + (lead.company || lead.name)
      );
      var body = encodeURIComponent(
        [
          "Name: " + lead.name,
          "Work Email: " + lead.email,
          "Company: " + (lead.company || "N/A"),
          "Team Size: " + lead.team_size,
          "Repo URL: " + (lead.repo_url || "N/A"),
          "",
          "Current onboarding blockers:",
          lead.message || "N/A",
        ].join("\n")
      );

      window.location.href =
        "mailto:mshabeeburrahman786@gmail.com?subject=" + subject + "&body=" + body;

      if (status) {
        status.textContent =
          "Email draft opened. If nothing opens, send details to mshabeeburrahman786@gmail.com.";
      }
    });
  }

  track("page_view");
  bindClickTracking();
  bindScrollDepthTracking();
  bindLeadForm();
})();
