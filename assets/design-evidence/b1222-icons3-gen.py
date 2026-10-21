import math, sys
D = sys.argv[1]
BG, G1, G2, G3 = "#18181B", "#6D6D6F", "#A4A4A5", "#FAFAFA"
def app(body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><rect x="100" y="100" width="824" height="824" rx="185" fill="{BG}"/>{body}</svg>'
def mb(body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18">{body}</svg>'
def hexpts(cx, cy, r):
    return " ".join(f"{cx+r*math.cos(math.radians(a)):.2f},{cy+r*math.sin(math.radians(a)):.2f}" for a in (-90,-30,30,90,150,210))
def w(n, a, m):
    open(f"{D}/{n}/app.svg", "w").write(app(a)); open(f"{D}/{n}/menubar.svg", "w").write(mb(m))

# 4 推桿：三條軌道、旋鈕停在你推好的高度
def faders(s, ox, oy, track, knobs, rx_t, rx_k):
    out = ""
    for i, (y, c) in enumerate(zip((0.62, 0.18, 0.40), knobs)):
        x = ox + i*s*1.0
        out += f'<rect x="{x+s*0.36:.2f}" y="{oy:.2f}" width="{s*0.28:.2f}" height="{s*2.2:.2f}" rx="{rx_t}" fill="{track}"/>'
        out += f'<rect x="{x+s*0.08:.2f}" y="{oy+y*s*2.2:.2f}" width="{s*0.84:.2f}" height="{s*0.48:.2f}" rx="{rx_k}" fill="{c}"/>'
    return out
w(4, faders(200, 212, 290, "#3A3A3D", (G1, G3, G2), 28, 40),
     faders(5.4, 0.9, 1.0, "#000", ("#000",)*3, 0.5, 1.0))

# 5 六角準心：Jetto 六角環，中心一顆白點＝被鎖定的那一個
w(5, f'<polygon points="{hexpts(512,512,290)}" fill="none" stroke="{G1}" stroke-width="70" stroke-linejoin="round"/>'
     f'<polygon points="{hexpts(512,512,175)}" fill="none" stroke="{G2}" stroke-width="56" stroke-linejoin="round"/>'
     f'<circle cx="512" cy="512" r="72" fill="{G3}"/>',
     f'<polygon points="{hexpts(9,9,7.4)}" fill="none" stroke="#000" stroke-width="1.6" stroke-linejoin="round"/>'
     f'<circle cx="9" cy="9" r="2.6" fill="#000"/>')

# 6 拉回：一道弧線繞回來，把外面那顆點帶回中心的座位
def pull(cx, cy, r, sw, ah, c_arc, c_seat, c_dot, dot_r, seat_w, seat_h, seat_rx):
    a0, a1 = -60, 210
    p = lambda a: (cx+r*math.cos(math.radians(a)), cy+r*math.sin(math.radians(a)))
    (x0,y0),(x1,y1) = p(a0), p(a1)
    arcd = f'<path d="M{x0:.2f},{y0:.2f} A{r},{r} 0 1 1 {x1:.2f},{y1:.2f}" fill="none" stroke="{c_arc}" stroke-width="{sw}" stroke-linecap="round"/>'
    # 箭頭在弧尾，沿切線方向（順時針）
    t = math.radians(a1 + 90); n = math.radians(a1)
    tip = (x1 + ah*math.cos(t)*0.9, y1 + ah*math.sin(t)*0.9)
    l = (x1 + ah*0.75*math.cos(n), y1 + ah*0.75*math.sin(n)); rr = (x1 - ah*0.75*math.cos(n), y1 - ah*0.75*math.sin(n))
    head = f'<polygon points="{tip[0]:.2f},{tip[1]:.2f} {l[0]:.2f},{l[1]:.2f} {rr[0]:.2f},{rr[1]:.2f}" fill="{c_arc}" stroke="{c_arc}" stroke-width="{sw*0.3}" stroke-linejoin="round"/>'
    seat = f'<rect x="{cx-seat_w/2:.2f}" y="{cy-seat_h/2:.2f}" width="{seat_w}" height="{seat_h}" rx="{seat_rx}" fill="{c_seat}"/>'
    dot = f'<circle cx="{cx:.2f}" cy="{cy:.2f}" r="{dot_r}" fill="{c_dot}"/>'
    return arcd + head + seat + dot
w(6, pull(512, 512, 250, 70, 110, G2, G1, G3, 62, 200, 200, 56),
     pull(9, 9.4, 6.6, 1.7, 3.0, "#000", "#000", "#000", 0, 4.6, 4.6, 1.3))

# 7 圖釘：側視圖釘斜釘進底板，把狀態釘在原位
def pin(s, cx, cy, rot, c_cap, c_body, c_disc, c_needle, plate):
    k = lambda v: v*s
    g = (f'<rect x="{cx-k(110)}" y="{cy-k(260)}" width="{k(220)}" height="{k(80)}" rx="{k(40)}" fill="{c_cap}"/>'
         f'<path d="M{cx-k(70)},{cy-k(190)} L{cx+k(70)},{cy-k(190)} L{cx+k(90)},{cy-k(10)} L{cx-k(90)},{cy-k(10)} Z" fill="{c_body}"/>'
         f'<rect x="{cx-k(160)}" y="{cy-k(30)}" width="{k(320)}" height="{k(60)}" rx="{k(30)}" fill="{c_disc}"/>'
         f'<rect x="{cx-k(18)}" y="{cy+k(20)}" width="{k(36)}" height="{k(210)}" rx="{k(18)}" fill="{c_needle}"/>')
    return plate + f'<g transform="rotate({rot} {cx} {cy})">{g}</g>'
w(7, pin(1.0, 512, 470, 30, G3, G2, G3, G1,
         f'<rect x="250" y="730" width="524" height="56" rx="28" fill="#3A3A3D"/>'),
     pin(0.031, 8.6, 9.4, 30, "#000", "#000", "#000", "#000", ""))

# 8 優先排序：三條橫向膠囊由長到短，最上一條最亮、左端一顆釘＝第一順位被釘住
def ranks(x0, y0, h, gap, lens, cols, pin_r, pin_c, pin_dx):
    out = ""
    for i, (L, c) in enumerate(zip(lens, cols)):
        out += f'<rect x="{x0}" y="{y0+i*(h+gap):.2f}" width="{L}" height="{h}" rx="{h/2}" fill="{c}"/>'
    out += f'<circle cx="{x0+pin_dx:.2f}" cy="{y0+h/2:.2f}" r="{pin_r}" fill="{pin_c}"/>'
    return out
w(8, ranks(232, 262, 140, 70, (560, 420, 280), (G3, G2, G1), 34, BG, 70),
     f'<mask id="m"><rect width="18" height="18" fill="#fff"/><circle cx="3.6" cy="3.6" r="1.1" fill="#000"/></mask>'
     f'<g mask="url(#m)">' + ranks(1.0, 1.6, 4.0, 1.6, (16.0, 11.6, 7.2), ("#000",)*3, 0, "#000", 0) + '</g>')
