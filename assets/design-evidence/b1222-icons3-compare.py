import base64, sys, subprocess
O = sys.argv[1]
items = [
 (1, "船栓", "把設定繫緊，不讓它漂走"),
 (2, "釘住的空間", "視窗與桌面各自釘在原位"),
 (3, "六角配置", "整台 Mac 的配置格被守住"),
 (4, "推桿", "每個裝置停在你推好的位置"),
 (5, "六角準心", "偏離了就拉回第一順位"),
 (6, "拉回", "被 iPhone 搶走的耳機帶回來"),
 (7, "圖釘", "狀態釘住，系統亂動也不跑"),
 (8, "優先排序", "第一順位永遠排在最上面"),
]
b64 = lambda p: base64.b64encode(open(p, "rb").read()).decode()
W, CW, CH, TOP = 1080, 540, 620, 110
rows = (len(items)+1)//2
H = TOP + rows*CH + 20
F = 'font-family="PingFang TC, PingFang HK, sans-serif"'
out = [f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}">',
       f'<rect width="{W}" height="{H}" fill="#FFFFFF"/>',
       f'<text x="{W/2}" y="64" text-anchor="middle" font-size="40" font-weight="600" {F} fill="#111">Cleat 圖示 8 款，按編號選</text>']
for i, (n, name, mean) in enumerate(items):
    x = (i % 2)*CW; y = TOP + (i//2)*CH
    a, m = b64(f"{O}/{n}/app-1024.png"), b64(f"{O}/{n}/menubar@2x.png")
    out += [f'<rect x="{x+16}" y="{y}" width="{CW-32}" height="{CH-20}" rx="24" fill="#F4F4F5"/>',
            f'<text x="{x+40}" y="{y+62}" font-size="52" font-weight="700" {F} fill="#111">{n}</text>',
            f'<text x="{x+92}" y="{y+58}" font-size="32" font-weight="600" {F} fill="#333">{name}</text>',
            f'<image x="{x+120}" y="{y+80}" width="300" height="300" xlink:href="data:image/png;base64,{a}"/>',
            # 選單列原尺寸：@2x 36px，淺列黑圖、深列白圖
            f'<rect x="{x+70}" y="{y+400}" width="190" height="56" rx="10" fill="#E6E6E8"/>',
            f'<image x="{x+147}" y="{y+410}" width="36" height="36" xlink:href="data:image/png;base64,{m}"/>',
            f'<rect x="{x+280}" y="{y+400}" width="190" height="56" rx="10" fill="#2A2A2D"/>',
            f'<g filter="url(#inv)"><image x="{x+357}" y="{y+410}" width="36" height="36" xlink:href="data:image/png;base64,{m}"/></g>',
            f'<text x="{x+CW/2}" y="{y+520}" text-anchor="middle" font-size="31" {F} fill="#111">{mean}</text>',
            f'<text x="{x+CW/2}" y="{y+566}" text-anchor="middle" font-size="20" {F} fill="#888">選單列原尺寸：淺色列｜深色列</text>']
out.insert(1, '<defs><filter id="inv"><feColorMatrix type="matrix" values="-1 0 0 0 1  0 -1 0 0 1  0 0 -1 0 1  0 0 0 1 0"/></filter></defs>')
out.append('</svg>')
svg = f"{O}/../b1222-icons3-work/compare.svg"
open(svg, "w").write("".join(out))
subprocess.run(["rsvg-convert", svg, "-o", f"{O}/b1222-icons3-compare.png"], check=True)
