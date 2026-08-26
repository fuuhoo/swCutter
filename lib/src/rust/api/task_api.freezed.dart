// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'task_api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$TaskEventKind {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEventKind);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'TaskEventKind()';
}


}

/// @nodoc
class $TaskEventKindCopyWith<$Res>  {
$TaskEventKindCopyWith(TaskEventKind _, $Res Function(TaskEventKind) __);
}


/// Adds pattern-matching-related methods to [TaskEventKind].
extension TaskEventKindPatterns on TaskEventKind {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( TaskEventKind_StatusChanged value)?  statusChanged,TResult Function( TaskEventKind_Started value)?  started,TResult Function( TaskEventKind_LevelStart value)?  levelStart,TResult Function( TaskEventKind_Progress value)?  progress,TResult Function( TaskEventKind_Finished value)?  finished,required TResult orElse(),}){
final _that = this;
switch (_that) {
case TaskEventKind_StatusChanged() when statusChanged != null:
return statusChanged(_that);case TaskEventKind_Started() when started != null:
return started(_that);case TaskEventKind_LevelStart() when levelStart != null:
return levelStart(_that);case TaskEventKind_Progress() when progress != null:
return progress(_that);case TaskEventKind_Finished() when finished != null:
return finished(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( TaskEventKind_StatusChanged value)  statusChanged,required TResult Function( TaskEventKind_Started value)  started,required TResult Function( TaskEventKind_LevelStart value)  levelStart,required TResult Function( TaskEventKind_Progress value)  progress,required TResult Function( TaskEventKind_Finished value)  finished,}){
final _that = this;
switch (_that) {
case TaskEventKind_StatusChanged():
return statusChanged(_that);case TaskEventKind_Started():
return started(_that);case TaskEventKind_LevelStart():
return levelStart(_that);case TaskEventKind_Progress():
return progress(_that);case TaskEventKind_Finished():
return finished(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( TaskEventKind_StatusChanged value)?  statusChanged,TResult? Function( TaskEventKind_Started value)?  started,TResult? Function( TaskEventKind_LevelStart value)?  levelStart,TResult? Function( TaskEventKind_Progress value)?  progress,TResult? Function( TaskEventKind_Finished value)?  finished,}){
final _that = this;
switch (_that) {
case TaskEventKind_StatusChanged() when statusChanged != null:
return statusChanged(_that);case TaskEventKind_Started() when started != null:
return started(_that);case TaskEventKind_LevelStart() when levelStart != null:
return levelStart(_that);case TaskEventKind_Progress() when progress != null:
return progress(_that);case TaskEventKind_Finished() when finished != null:
return finished(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String status)?  statusChanged,TResult Function( BigInt totalTiles)?  started,TResult Function( int level)?  levelStart,TResult Function( int level,  BigInt tilesDone,  BigInt totalTiles,  BigInt bytesWritten,  BigInt elapsedMs)?  progress,TResult Function( TaskSummary summary)?  finished,required TResult orElse(),}) {final _that = this;
switch (_that) {
case TaskEventKind_StatusChanged() when statusChanged != null:
return statusChanged(_that.status);case TaskEventKind_Started() when started != null:
return started(_that.totalTiles);case TaskEventKind_LevelStart() when levelStart != null:
return levelStart(_that.level);case TaskEventKind_Progress() when progress != null:
return progress(_that.level,_that.tilesDone,_that.totalTiles,_that.bytesWritten,_that.elapsedMs);case TaskEventKind_Finished() when finished != null:
return finished(_that.summary);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String status)  statusChanged,required TResult Function( BigInt totalTiles)  started,required TResult Function( int level)  levelStart,required TResult Function( int level,  BigInt tilesDone,  BigInt totalTiles,  BigInt bytesWritten,  BigInt elapsedMs)  progress,required TResult Function( TaskSummary summary)  finished,}) {final _that = this;
switch (_that) {
case TaskEventKind_StatusChanged():
return statusChanged(_that.status);case TaskEventKind_Started():
return started(_that.totalTiles);case TaskEventKind_LevelStart():
return levelStart(_that.level);case TaskEventKind_Progress():
return progress(_that.level,_that.tilesDone,_that.totalTiles,_that.bytesWritten,_that.elapsedMs);case TaskEventKind_Finished():
return finished(_that.summary);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String status)?  statusChanged,TResult? Function( BigInt totalTiles)?  started,TResult? Function( int level)?  levelStart,TResult? Function( int level,  BigInt tilesDone,  BigInt totalTiles,  BigInt bytesWritten,  BigInt elapsedMs)?  progress,TResult? Function( TaskSummary summary)?  finished,}) {final _that = this;
switch (_that) {
case TaskEventKind_StatusChanged() when statusChanged != null:
return statusChanged(_that.status);case TaskEventKind_Started() when started != null:
return started(_that.totalTiles);case TaskEventKind_LevelStart() when levelStart != null:
return levelStart(_that.level);case TaskEventKind_Progress() when progress != null:
return progress(_that.level,_that.tilesDone,_that.totalTiles,_that.bytesWritten,_that.elapsedMs);case TaskEventKind_Finished() when finished != null:
return finished(_that.summary);case _:
  return null;

}
}

}

/// @nodoc


class TaskEventKind_StatusChanged extends TaskEventKind {
  const TaskEventKind_StatusChanged({required this.status}): super._();
  

 final  String status;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEventKind_StatusChangedCopyWith<TaskEventKind_StatusChanged> get copyWith => _$TaskEventKind_StatusChangedCopyWithImpl<TaskEventKind_StatusChanged>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEventKind_StatusChanged&&(identical(other.status, status) || other.status == status));
}


@override
int get hashCode => Object.hash(runtimeType,status);

@override
String toString() {
  return 'TaskEventKind.statusChanged(status: $status)';
}


}

/// @nodoc
abstract mixin class $TaskEventKind_StatusChangedCopyWith<$Res> implements $TaskEventKindCopyWith<$Res> {
  factory $TaskEventKind_StatusChangedCopyWith(TaskEventKind_StatusChanged value, $Res Function(TaskEventKind_StatusChanged) _then) = _$TaskEventKind_StatusChangedCopyWithImpl;
@useResult
$Res call({
 String status
});




}
/// @nodoc
class _$TaskEventKind_StatusChangedCopyWithImpl<$Res>
    implements $TaskEventKind_StatusChangedCopyWith<$Res> {
  _$TaskEventKind_StatusChangedCopyWithImpl(this._self, this._then);

  final TaskEventKind_StatusChanged _self;
  final $Res Function(TaskEventKind_StatusChanged) _then;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? status = null,}) {
  return _then(TaskEventKind_StatusChanged(
status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class TaskEventKind_Started extends TaskEventKind {
  const TaskEventKind_Started({required this.totalTiles}): super._();
  

 final  BigInt totalTiles;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEventKind_StartedCopyWith<TaskEventKind_Started> get copyWith => _$TaskEventKind_StartedCopyWithImpl<TaskEventKind_Started>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEventKind_Started&&(identical(other.totalTiles, totalTiles) || other.totalTiles == totalTiles));
}


@override
int get hashCode => Object.hash(runtimeType,totalTiles);

@override
String toString() {
  return 'TaskEventKind.started(totalTiles: $totalTiles)';
}


}

/// @nodoc
abstract mixin class $TaskEventKind_StartedCopyWith<$Res> implements $TaskEventKindCopyWith<$Res> {
  factory $TaskEventKind_StartedCopyWith(TaskEventKind_Started value, $Res Function(TaskEventKind_Started) _then) = _$TaskEventKind_StartedCopyWithImpl;
@useResult
$Res call({
 BigInt totalTiles
});




}
/// @nodoc
class _$TaskEventKind_StartedCopyWithImpl<$Res>
    implements $TaskEventKind_StartedCopyWith<$Res> {
  _$TaskEventKind_StartedCopyWithImpl(this._self, this._then);

  final TaskEventKind_Started _self;
  final $Res Function(TaskEventKind_Started) _then;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? totalTiles = null,}) {
  return _then(TaskEventKind_Started(
totalTiles: null == totalTiles ? _self.totalTiles : totalTiles // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class TaskEventKind_LevelStart extends TaskEventKind {
  const TaskEventKind_LevelStart({required this.level}): super._();
  

 final  int level;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEventKind_LevelStartCopyWith<TaskEventKind_LevelStart> get copyWith => _$TaskEventKind_LevelStartCopyWithImpl<TaskEventKind_LevelStart>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEventKind_LevelStart&&(identical(other.level, level) || other.level == level));
}


@override
int get hashCode => Object.hash(runtimeType,level);

@override
String toString() {
  return 'TaskEventKind.levelStart(level: $level)';
}


}

/// @nodoc
abstract mixin class $TaskEventKind_LevelStartCopyWith<$Res> implements $TaskEventKindCopyWith<$Res> {
  factory $TaskEventKind_LevelStartCopyWith(TaskEventKind_LevelStart value, $Res Function(TaskEventKind_LevelStart) _then) = _$TaskEventKind_LevelStartCopyWithImpl;
@useResult
$Res call({
 int level
});




}
/// @nodoc
class _$TaskEventKind_LevelStartCopyWithImpl<$Res>
    implements $TaskEventKind_LevelStartCopyWith<$Res> {
  _$TaskEventKind_LevelStartCopyWithImpl(this._self, this._then);

  final TaskEventKind_LevelStart _self;
  final $Res Function(TaskEventKind_LevelStart) _then;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? level = null,}) {
  return _then(TaskEventKind_LevelStart(
level: null == level ? _self.level : level // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class TaskEventKind_Progress extends TaskEventKind {
  const TaskEventKind_Progress({required this.level, required this.tilesDone, required this.totalTiles, required this.bytesWritten, required this.elapsedMs}): super._();
  

 final  int level;
 final  BigInt tilesDone;
 final  BigInt totalTiles;
 final  BigInt bytesWritten;
 final  BigInt elapsedMs;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEventKind_ProgressCopyWith<TaskEventKind_Progress> get copyWith => _$TaskEventKind_ProgressCopyWithImpl<TaskEventKind_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEventKind_Progress&&(identical(other.level, level) || other.level == level)&&(identical(other.tilesDone, tilesDone) || other.tilesDone == tilesDone)&&(identical(other.totalTiles, totalTiles) || other.totalTiles == totalTiles)&&(identical(other.bytesWritten, bytesWritten) || other.bytesWritten == bytesWritten)&&(identical(other.elapsedMs, elapsedMs) || other.elapsedMs == elapsedMs));
}


@override
int get hashCode => Object.hash(runtimeType,level,tilesDone,totalTiles,bytesWritten,elapsedMs);

@override
String toString() {
  return 'TaskEventKind.progress(level: $level, tilesDone: $tilesDone, totalTiles: $totalTiles, bytesWritten: $bytesWritten, elapsedMs: $elapsedMs)';
}


}

/// @nodoc
abstract mixin class $TaskEventKind_ProgressCopyWith<$Res> implements $TaskEventKindCopyWith<$Res> {
  factory $TaskEventKind_ProgressCopyWith(TaskEventKind_Progress value, $Res Function(TaskEventKind_Progress) _then) = _$TaskEventKind_ProgressCopyWithImpl;
@useResult
$Res call({
 int level, BigInt tilesDone, BigInt totalTiles, BigInt bytesWritten, BigInt elapsedMs
});




}
/// @nodoc
class _$TaskEventKind_ProgressCopyWithImpl<$Res>
    implements $TaskEventKind_ProgressCopyWith<$Res> {
  _$TaskEventKind_ProgressCopyWithImpl(this._self, this._then);

  final TaskEventKind_Progress _self;
  final $Res Function(TaskEventKind_Progress) _then;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? level = null,Object? tilesDone = null,Object? totalTiles = null,Object? bytesWritten = null,Object? elapsedMs = null,}) {
  return _then(TaskEventKind_Progress(
level: null == level ? _self.level : level // ignore: cast_nullable_to_non_nullable
as int,tilesDone: null == tilesDone ? _self.tilesDone : tilesDone // ignore: cast_nullable_to_non_nullable
as BigInt,totalTiles: null == totalTiles ? _self.totalTiles : totalTiles // ignore: cast_nullable_to_non_nullable
as BigInt,bytesWritten: null == bytesWritten ? _self.bytesWritten : bytesWritten // ignore: cast_nullable_to_non_nullable
as BigInt,elapsedMs: null == elapsedMs ? _self.elapsedMs : elapsedMs // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class TaskEventKind_Finished extends TaskEventKind {
  const TaskEventKind_Finished({required this.summary}): super._();
  

 final  TaskSummary summary;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEventKind_FinishedCopyWith<TaskEventKind_Finished> get copyWith => _$TaskEventKind_FinishedCopyWithImpl<TaskEventKind_Finished>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEventKind_Finished&&(identical(other.summary, summary) || other.summary == summary));
}


@override
int get hashCode => Object.hash(runtimeType,summary);

@override
String toString() {
  return 'TaskEventKind.finished(summary: $summary)';
}


}

/// @nodoc
abstract mixin class $TaskEventKind_FinishedCopyWith<$Res> implements $TaskEventKindCopyWith<$Res> {
  factory $TaskEventKind_FinishedCopyWith(TaskEventKind_Finished value, $Res Function(TaskEventKind_Finished) _then) = _$TaskEventKind_FinishedCopyWithImpl;
@useResult
$Res call({
 TaskSummary summary
});




}
/// @nodoc
class _$TaskEventKind_FinishedCopyWithImpl<$Res>
    implements $TaskEventKind_FinishedCopyWith<$Res> {
  _$TaskEventKind_FinishedCopyWithImpl(this._self, this._then);

  final TaskEventKind_Finished _self;
  final $Res Function(TaskEventKind_Finished) _then;

/// Create a copy of TaskEventKind
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? summary = null,}) {
  return _then(TaskEventKind_Finished(
summary: null == summary ? _self.summary : summary // ignore: cast_nullable_to_non_nullable
as TaskSummary,
  ));
}


}

// dart format on
