<?php

function swap(&$left, &$right): void
{
    [$left, $right] = [$right, $left];
}
