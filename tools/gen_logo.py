# Генерит лого BlackHole: чёрная дыра с закрученными рукавами аккреционного диска.
import math, sys, os

def arm(k, n_arms=3, turns=1.15, r0=96, r1=236, w0=30):
    pts_l, pts_r = [], []
    steps = 90
    for i in range(steps + 1):
        t = i / steps
        th = k * 2 * math.pi / n_arms + t * turns * 2 * math.pi
        r = r0 + (r1 - r0) * t ** 1.15
        w = w0 * (1 - t) ** 1.4 + 0.5
        cx, cy = 256 + r * math.cos(th), 256 + r * math.sin(th)
        # нормаль к спирали (приближённо радиальная)
        nx, ny = math.cos(th), math.sin(th)
        pts_l.append((cx - nx * w / 2, cy - ny * w / 2))
        pts_r.append((cx + nx * w / 2, cy + ny * w / 2))
    pts = pts_l + pts_r[::-1]
    return "M" + " L".join(f"{x:.1f} {y:.1f}" for x, y in pts) + " Z"

arms = "".join(f'<path d="{arm(k)}"/>' for k in range(3))

def svg(bg):
    back = '<rect width="512" height="512" rx="116" fill="url(#bg)"/>' if bg else ''
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
<defs>
<radialGradient id="bg" cx="50%" cy="45%" r="70%"><stop offset="0" stop-color="#1c1030"/><stop offset="1" stop-color="#05040a"/></radialGradient>
<radialGradient id="disk" cx="256" cy="256" r="240" gradientUnits="userSpaceOnUse">
<stop offset=".38" stop-color="#fff1d6"/><stop offset=".5" stop-color="#ffb066"/><stop offset=".68" stop-color="#ff6a3d"/><stop offset=".85" stop-color="#b14cff"/><stop offset="1" stop-color="#6a2cff" stop-opacity="0"/></radialGradient>
<radialGradient id="halo" cx="256" cy="256" r="150" gradientUnits="userSpaceOnUse">
<stop offset=".55" stop-color="#ffb066" stop-opacity="0"/><stop offset=".66" stop-color="#ffcf99" stop-opacity=".9"/><stop offset=".8" stop-color="#a855f7" stop-opacity=".45"/><stop offset="1" stop-color="#a855f7" stop-opacity="0"/></radialGradient>
<filter id="blur" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="9"/></filter>
</defs>
{back}
<circle cx="256" cy="256" r="150" fill="url(#halo)"/>
<g fill="url(#disk)" filter="url(#blur)" opacity=".85">{arms}</g>
<g fill="url(#disk)">{arms}</g>
<circle cx="256" cy="256" r="100" fill="none" stroke="#ffd9a8" stroke-width="7" filter="url(#blur)"/>
<circle cx="256" cy="256" r="98" fill="none" stroke="#fff3e0" stroke-width="2.5"/>
<circle cx="256" cy="256" r="95" fill="#000"/>
</svg>'''

out = sys.argv[1]
open(os.path.join(out, "logo-mark.svg"), "w").write(svg(False))
open(os.path.join(out, "icon.svg"), "w").write(svg(True))
