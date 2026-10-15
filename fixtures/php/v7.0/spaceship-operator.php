<?php

$order = 1 <=> 2;
usort($items, fn_name_placeholder());

function fn_name_placeholder()
{
    return function ($a, $b) {
        return $a['n'] <=> $b['n'];
    };
}
