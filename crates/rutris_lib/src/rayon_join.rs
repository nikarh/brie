#[macro_export]
#[doc(hidden)]
macro_rules! __join_implementation {
    ($len:expr; $($f:ident $r:ident $a:expr),*; $b:expr, $($c:expr,)*) => {
        $crate::__join_implementation!{$len + 1; $($f $r $a,)* f r $b; $($c,)* }
    };
    ($len:expr; $($f:ident $r:ident $a:expr),* ;) => {
        match ($(Some($a),)*) {
            ($(mut $f,)*) => {
                $(let mut $r = None;)*
                let array: [&mut (dyn FnMut() + Send); $len] = [
                    $(&mut || $r = Some((&mut $f).take().unwrap()())),*
                ];
                rayon::iter::ParallelIterator::for_each(
                    rayon::iter::IntoParallelIterator::into_par_iter(array),
                    |f| f(),
                );
                ($($r.unwrap(),)*)
            }
        }
    };
}

#[macro_export]
macro_rules! join {
    ($($($a:expr),+$(,)?)?) => {
        $crate::__join_implementation!{0;;$($($a,)+)?}
    };
}
