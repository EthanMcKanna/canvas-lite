"""Fake Canvas API for trying the UI without a real account.

    python3 dev/mock_canvas.py   # serves on :8765
    CANVAS_URL=http://localhost:8765 CANVAS_TOKEN=x cargo run --release
"""
import json, datetime as dt
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

now = dt.datetime.now(dt.timezone.utc)
monday = (now - dt.timedelta(days=now.astimezone().weekday())).replace(hour=23, minute=59, second=0, microsecond=0)
iso = lambda d: d.strftime("%Y-%m-%dT%H:%M:%SZ")

COURSES = [
    (101, "CS 3110: Functional Programming", "CS 3110", 93.4, "A"),
    (102, "MATH 2940: Linear Algebra", "MATH 2940", 87.1, "B+"),
    (103, "PHIL 1100: Intro to Ethics", "PHIL 1100", 95.0, "A"),
    (104, "ECON 1110: Microeconomics", "ECON 1110", 81.6, "B-"),
]
TITLES = ["Problem Set", "Reading Response", "Lab", "Quiz", "Essay Draft", "Discussion Post"]

def planner(start, end):
    items, n = [], 0
    for d in range(-7, 14):
        for i, c in enumerate(COURSES):
            if (d + i) % 3: continue
            n += 1
            when = monday + dt.timedelta(days=d, hours=-(i * 3))
            kind = "quiz" if n % 5 == 0 else "discussion_topic" if n % 7 == 0 else "assignment"
            sub = {"submitted": when < now and n % 4 != 0, "graded": when < now and n % 3 == 0,
                   "missing": when < now and n % 4 == 0, "late": False}
            if not (start <= iso(when) < end): continue
            items.append({"plannable_id": n, "plannable_type": kind, "plannable_date": iso(when),
                          "course_id": c[0], "context_name": c[1],
                          "html_url": f"/courses/{c[0]}/assignments/{n}",
                          "plannable": {"title": f"{TITLES[n % len(TITLES)]} {n}", "points_possible": 10 * (1 + n % 5)},
                          "submissions": sub, "planner_override": None})
    return items

def assignments(cid):
    out = []
    for k in range(-6, 8):
        due = monday + dt.timedelta(days=k * 2)
        past = due < now
        out.append({"id": cid * 100 + k + 10, "name": f"{TITLES[k % len(TITLES)]} {k + 7}", "course_id": cid,
                    "due_at": iso(due), "points_possible": 20, "html_url": f"/courses/{cid}/assignments/{k}",
                    "submission": {"workflow_state": "graded" if past else "unsubmitted",
                                   "score": (14 + (k * 7) % 7) if past and k % 3 else None,
                                   "submitted_at": iso(due) if past else None,
                                   "missing": past and k % 5 == 0, "late": past and k % 4 == 0}})
    return out

class H(BaseHTTPRequestHandler):
    def html(self, body, headers=()):
        self.send_response(200 if not headers else 302)
        for k, v in headers: self.send_header(k, v)
        self.send_header("Content-Type", "text/html"); self.end_headers(); self.wfile.write(body.encode())

    def authed(self):
        ok = self.headers.get("Authorization", "").startswith("Bearer ") or "canvas_session=s3cret" in self.headers.get("Cookie", "")
        if not ok: self.send_response(401); self.end_headers()
        return ok

    def do_GET(self):
        u = urlparse(self.path); p = u.path
        # Fake SSO: /login -> click -> sets session cookies -> dashboard.
        if p == "/login": return self.html('<meta http-equiv="refresh" content="1;url=/login/done"><h1>Mock school login</h1><a id="go" href="/login/done" style="font-size:40px">Sign in</a>')
        if p == "/login/done": return self.html("", [("Set-Cookie", "canvas_session=s3cret; Path=/; HttpOnly"), ("Set-Cookie", "_csrf_token=abc%2Bdef; Path=/"), ("Location", "/?login_success=1")])
        if p == "/": return self.html("<h1>Dashboard</h1>")
        if not self.authed(): return
        q = dict(x.split("=", 1) for x in u.query.split("&") if "=" in x)
        from urllib.parse import unquote
        q = {k: unquote(v) for k, v in q.items()}
        if p == "/api/v1/users/self": body = {"id": 1, "name": "Test Student"}
        elif p == "/api/v1/users/self/favorites/courses":
            body = [{"id": i, "name": n, "course_code": c, "enrollments": [{"type": "student", "computed_current_score": s, "computed_current_grade": g}]} for i, n, c, s, g in COURSES]
        elif p == "/api/v1/users/self/colors": body = {"custom_colors": {"course_101": "#E2574C", "course_102": "#3B82F6"}}
        elif p == "/api/v1/planner/items": body = planner(q["start_date"][:19] + "Z", q["end_date"][:19] + "Z")
        elif p == "/api/v1/users/self/missing_submissions":
            body = [{"id": 9001, "name": "Lab Report 2", "course_id": 102, "due_at": iso(monday - dt.timedelta(days=9)), "points_possible": 30, "html_url": "/courses/102/assignments/9001"}]
        elif p.endswith("/assignments"): body = assignments(int(p.split("/")[4]))
        elif p.endswith("/modules"):
            body = [{"id": m, "name": f"Week {m}: Topic {m}", "items": [
                {"title": "Lecture notes", "type": "SubHeader", "indent": 0},
                {"title": f"Slides {m}.pdf", "type": "File", "indent": 1, "html_url": "/files/1"},
                {"title": f"Problem Set {m}", "type": "Assignment", "indent": 1, "html_url": "/a/1"}]} for m in range(1, 6)]
        elif p == "/api/v1/announcements":
            body = [{"id": k, "title": f"Office hours moved (week {k})", "posted_at": iso(now - dt.timedelta(days=k * 3)),
                     "message": "<p>Hi all,</p><p>Office hours this week are <b>Thursday 3&ndash;5pm</b> in Gates 310.</p><ul><li>Bring questions on PS4</li><li>Midterm review Friday</li></ul>",
                     "html_url": "/x", "author": {"display_name": "Prof. Example"}} for k in range(1, 5)]
        elif p.startswith("/api/v1/courses/"):
            body = {"id": 1, "syllabus_body": "<h2>Course Policies</h2><p>Grading: 40% problem sets, 25% midterm, 35% final.</p><p>Late work: 10%/day.</p>"}
        else: self.send_response(404); self.end_headers(); return
        data = json.dumps(body).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers(); self.wfile.write(data)
    def do_POST(self):
        if not self.authed(): return
        if "Cookie" in self.headers and self.headers.get("X-CSRF-Token") != "abc+def":
            self.send_response(422); self.end_headers(); return
        n = int(self.headers.get("Content-Length", 0)); body = json.loads(self.rfile.read(n) or b"{}")
        data = json.dumps({"id": 500 + int(body.get("plannable_id", 0)), "marked_complete": body.get("marked_complete", True)}).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers(); self.wfile.write(data)
    do_PUT = do_POST
    def log_message(self, *a): pass

ThreadingHTTPServer(("127.0.0.1", 8765), H).serve_forever()
