import simplecadapi as scad
base = scad.make_box_rsolid(60.0, 36.0, 8.0)
hole = scad.make_cylinder_rsolid(5.0, 14.0)
part = scad.cut_rsolid(base, hole)
boss = scad.make_cylinder_rsolid(8.0, 7.0)
part = scad.union_rsolid(part, boss)
print(f'volume={part.get_volume():.2f}, faces={len(part.get_faces())}')
