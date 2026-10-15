<?php

$label = match ($code) {
    200, 201 => 'ok',
    404 => 'missing',
    default => 'other',
};
