// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'alpha.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$AlphaMode {
  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is AlphaMode);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'AlphaMode()';
  }
}

/// @nodoc
class $AlphaModeCopyWith<$Res> {
  $AlphaModeCopyWith(AlphaMode _, $Res Function(AlphaMode) __);
}

/// Adds pattern-matching-related methods to [AlphaMode].
extension AlphaModePatterns on AlphaMode {
  /// A variant of `map` that fallback to returning `orElse`.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case _:
  ///     return orElse();
  /// }
  /// ```

  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(AlphaMode_Keep value)? keep,
    TResult Function(AlphaMode_Threshold value)? threshold,
    TResult Function(AlphaMode_ColorKey value)? colorKey,
    required TResult orElse(),
  }) {
    final _that = this;
    switch (_that) {
      case AlphaMode_Keep() when keep != null:
        return keep(_that);
      case AlphaMode_Threshold() when threshold != null:
        return threshold(_that);
      case AlphaMode_ColorKey() when colorKey != null:
        return colorKey(_that);
      case _:
        return orElse();
    }
  }

  /// A `switch`-like method, using callbacks.
  ///
  /// Callbacks receives the raw object, upcasted.
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case final Subclass2 value:
  ///     return ...;
  /// }
  /// ```

  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(AlphaMode_Keep value) keep,
    required TResult Function(AlphaMode_Threshold value) threshold,
    required TResult Function(AlphaMode_ColorKey value) colorKey,
  }) {
    final _that = this;
    switch (_that) {
      case AlphaMode_Keep():
        return keep(_that);
      case AlphaMode_Threshold():
        return threshold(_that);
      case AlphaMode_ColorKey():
        return colorKey(_that);
    }
  }

  /// A variant of `map` that fallback to returning `null`.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case _:
  ///     return null;
  /// }
  /// ```

  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(AlphaMode_Keep value)? keep,
    TResult? Function(AlphaMode_Threshold value)? threshold,
    TResult? Function(AlphaMode_ColorKey value)? colorKey,
  }) {
    final _that = this;
    switch (_that) {
      case AlphaMode_Keep() when keep != null:
        return keep(_that);
      case AlphaMode_Threshold() when threshold != null:
        return threshold(_that);
      case AlphaMode_ColorKey() when colorKey != null:
        return colorKey(_that);
      case _:
        return null;
    }
  }

  /// A variant of `when` that fallback to an `orElse` callback.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case _:
  ///     return orElse();
  /// }
  /// ```

  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function()? keep,
    TResult Function(int below)? threshold,
    TResult Function(int r, int g, int b, int tolerance)? colorKey,
    required TResult orElse(),
  }) {
    final _that = this;
    switch (_that) {
      case AlphaMode_Keep() when keep != null:
        return keep();
      case AlphaMode_Threshold() when threshold != null:
        return threshold(_that.below);
      case AlphaMode_ColorKey() when colorKey != null:
        return colorKey(_that.r, _that.g, _that.b, _that.tolerance);
      case _:
        return orElse();
    }
  }

  /// A `switch`-like method, using callbacks.
  ///
  /// As opposed to `map`, this offers destructuring.
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case Subclass2(:final field2):
  ///     return ...;
  /// }
  /// ```

  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function() keep,
    required TResult Function(int below) threshold,
    required TResult Function(int r, int g, int b, int tolerance) colorKey,
  }) {
    final _that = this;
    switch (_that) {
      case AlphaMode_Keep():
        return keep();
      case AlphaMode_Threshold():
        return threshold(_that.below);
      case AlphaMode_ColorKey():
        return colorKey(_that.r, _that.g, _that.b, _that.tolerance);
    }
  }

  /// A variant of `when` that fallback to returning `null`
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case _:
  ///     return null;
  /// }
  /// ```

  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function()? keep,
    TResult? Function(int below)? threshold,
    TResult? Function(int r, int g, int b, int tolerance)? colorKey,
  }) {
    final _that = this;
    switch (_that) {
      case AlphaMode_Keep() when keep != null:
        return keep();
      case AlphaMode_Threshold() when threshold != null:
        return threshold(_that.below);
      case AlphaMode_ColorKey() when colorKey != null:
        return colorKey(_that.r, _that.g, _that.b, _that.tolerance);
      case _:
        return null;
    }
  }
}

/// @nodoc

class AlphaMode_Keep extends AlphaMode {
  const AlphaMode_Keep() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is AlphaMode_Keep);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'AlphaMode.keep()';
  }
}

/// @nodoc

class AlphaMode_Threshold extends AlphaMode {
  const AlphaMode_Threshold({required this.below}) : super._();

  final int below;

  /// Create a copy of AlphaMode
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $AlphaMode_ThresholdCopyWith<AlphaMode_Threshold> get copyWith =>
      _$AlphaMode_ThresholdCopyWithImpl<AlphaMode_Threshold>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is AlphaMode_Threshold &&
            (identical(other.below, below) || other.below == below));
  }

  @override
  int get hashCode => Object.hash(runtimeType, below);

  @override
  String toString() {
    return 'AlphaMode.threshold(below: $below)';
  }
}

/// @nodoc
abstract mixin class $AlphaMode_ThresholdCopyWith<$Res>
    implements $AlphaModeCopyWith<$Res> {
  factory $AlphaMode_ThresholdCopyWith(
          AlphaMode_Threshold value, $Res Function(AlphaMode_Threshold) _then) =
      _$AlphaMode_ThresholdCopyWithImpl;
  @useResult
  $Res call({int below});
}

/// @nodoc
class _$AlphaMode_ThresholdCopyWithImpl<$Res>
    implements $AlphaMode_ThresholdCopyWith<$Res> {
  _$AlphaMode_ThresholdCopyWithImpl(this._self, this._then);

  final AlphaMode_Threshold _self;
  final $Res Function(AlphaMode_Threshold) _then;

  /// Create a copy of AlphaMode
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? below = null,
  }) {
    return _then(AlphaMode_Threshold(
      below: null == below
          ? _self.below
          : below // ignore: cast_nullable_to_non_nullable
              as int,
    ));
  }
}

/// @nodoc

class AlphaMode_ColorKey extends AlphaMode {
  const AlphaMode_ColorKey(
      {required this.r,
      required this.g,
      required this.b,
      required this.tolerance})
      : super._();

  final int r;
  final int g;
  final int b;
  final int tolerance;

  /// Create a copy of AlphaMode
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $AlphaMode_ColorKeyCopyWith<AlphaMode_ColorKey> get copyWith =>
      _$AlphaMode_ColorKeyCopyWithImpl<AlphaMode_ColorKey>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is AlphaMode_ColorKey &&
            (identical(other.r, r) || other.r == r) &&
            (identical(other.g, g) || other.g == g) &&
            (identical(other.b, b) || other.b == b) &&
            (identical(other.tolerance, tolerance) ||
                other.tolerance == tolerance));
  }

  @override
  int get hashCode => Object.hash(runtimeType, r, g, b, tolerance);

  @override
  String toString() {
    return 'AlphaMode.colorKey(r: $r, g: $g, b: $b, tolerance: $tolerance)';
  }
}

/// @nodoc
abstract mixin class $AlphaMode_ColorKeyCopyWith<$Res>
    implements $AlphaModeCopyWith<$Res> {
  factory $AlphaMode_ColorKeyCopyWith(
          AlphaMode_ColorKey value, $Res Function(AlphaMode_ColorKey) _then) =
      _$AlphaMode_ColorKeyCopyWithImpl;
  @useResult
  $Res call({int r, int g, int b, int tolerance});
}

/// @nodoc
class _$AlphaMode_ColorKeyCopyWithImpl<$Res>
    implements $AlphaMode_ColorKeyCopyWith<$Res> {
  _$AlphaMode_ColorKeyCopyWithImpl(this._self, this._then);

  final AlphaMode_ColorKey _self;
  final $Res Function(AlphaMode_ColorKey) _then;

  /// Create a copy of AlphaMode
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? r = null,
    Object? g = null,
    Object? b = null,
    Object? tolerance = null,
  }) {
    return _then(AlphaMode_ColorKey(
      r: null == r
          ? _self.r
          : r // ignore: cast_nullable_to_non_nullable
              as int,
      g: null == g
          ? _self.g
          : g // ignore: cast_nullable_to_non_nullable
              as int,
      b: null == b
          ? _self.b
          : b // ignore: cast_nullable_to_non_nullable
              as int,
      tolerance: null == tolerance
          ? _self.tolerance
          : tolerance // ignore: cast_nullable_to_non_nullable
              as int,
    ));
  }
}

// dart format on
