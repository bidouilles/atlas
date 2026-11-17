import 'shapes.dart';

Square double_(Square s) => Square(s.side * 2);

double sum() => total([Square.unit(), double_(const Square(2))]);
