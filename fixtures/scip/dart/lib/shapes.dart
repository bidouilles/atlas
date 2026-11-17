/// Shapes, for the scip-dart fixture.
library;

enum _Unit { mm, cm }

/// A shape with an area.
abstract class Shape {
  const Shape();

  double get area;
}

class Square extends Shape {
  const Square(this.side);

  Square.unit() : side = 1;

  factory Square.fromJson(Map<String, Object?> json) =>
      Square(json['side'] as double);

  final double side;

  @override
  double get area => side * side;
}

class Box<TRes> {
  Box(this.value);

  final TRes value;
}

extension ShapeList on List<Shape> {
  double get total => fold(0, (sum, s) => sum + s.area);
}

String _label(_Unit u) => u.name;

double total(List<Shape> shapes) => shapes.total;
